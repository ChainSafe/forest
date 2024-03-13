// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use crate::blocks::{CachingBlockHeader, Tipset, TipsetKey};
use crate::chain::index::ResolveNullTipset;
use crate::chain::{ChainStore, HeadChange};
use crate::cid_collections::CidHashSet;
use crate::lotus_json::LotusJson;
use crate::message::ChainMessage;
use crate::rpc::error::JsonRpcError;
use crate::rpc_api::data_types::{ApiHeadChange, ApiMessage, ApiReceipt};
use crate::rpc_api::{
    chain_api::*,
    data_types::{ApiTipsetKey, BlockMessages, RPCState},
};
use crate::utils::io::VoidAsyncWriter;
use anyhow::{Context as _, Result};
use cid::Cid;
use fil_actors_shared::fvm_ipld_amt::Amtv0 as Amt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use hex::ToHex;
use jsonrpsee::types::error::ErrorObjectOwned;
use once_cell::sync::Lazy;
use sha2::Sha256;
use std::sync::Arc;
use tokio::sync::{
    broadcast::{self, Receiver as Subscriber},
    Mutex,
};

use super::reflect::SelfDescribingModule;

type Ctx<T> = Arc<Arc<RPCState<T>>>;

pub fn serve<BS: Blockstore + Send + Sync + 'static>(
    module: &mut SelfDescribingModule<Arc<RPCState<BS>>>,
) {
    {
        module
            .serve(
                CHAIN_GET_MESSAGE,
                ["msg_cid"],
                |ctx: Ctx<BS>, LotusJson(msg_cid)| async move {
                    let chain_message: ChainMessage = ctx
                        .state_manager
                        .blockstore()
                        .get_cbor(&msg_cid)?
                        .with_context(|| format!("can't find message with cid {msg_cid}"))?;
                    Ok(LotusJson(match chain_message {
                        ChainMessage::Signed(m) => m.into_message(),
                        ChainMessage::Unsigned(m) => m,
                    }))
                },
            )
            .serve(
                CHAIN_GET_PARENT_MESSAGES,
                ["block_cid"],
                |ctx: Ctx<BS>, LotusJson(block_cid)| async move {
                    let store = ctx.state_manager.blockstore();
                    let block_header: CachingBlockHeader = store
                        .get_cbor(&block_cid)?
                        .with_context(|| format!("can't find block header with cid {block_cid}"))?;
                    if block_header.epoch == 0 {
                        Ok(LotusJson(vec![]))
                    } else {
                        let parent_tipset = Tipset::load_required(store, &block_header.parents)?;
                        let messages = load_api_messages_from_tipset(store, &parent_tipset)?;
                        Ok(LotusJson(messages))
                    }
                },
            )
            .serve(
                CHAIN_GET_MESSAGES_IN_TIPSET,
                ["tsk"],
                |ctx: Ctx<BS>, LotusJson(tsk)| async move {
                    let store = ctx.chain_store.blockstore();
                    let tipset = Tipset::load_required(store, &tsk)?;
                    let messages = load_api_messages_from_tipset(store, &tipset)?;
                    Ok(LotusJson(messages))
                },
            )
            .serve(
                CHAIN_READ_OBJ,
                ["obj_cid"],
                |ctx: Ctx<BS>, LotusJson(obj_cid)| async move {
                    let bytes = ctx
                        .state_manager
                        .blockstore()
                        .get(&obj_cid)?
                        .context("can't find object with that cid")?;
                    Ok(LotusJson(bytes))
                },
            )
            .serve(
                CHAIN_HAS_OBJ,
                ["cid"],
                |ctx: Ctx<BS>, LotusJson(cid)| async move {
                    Ok(ctx.state_manager.blockstore().get(&cid)?.is_some())
                },
            )
            .serve(
                CHAIN_GET_PATH,
                ["from", "to"],
                |ctx: Ctx<BS>, LotusJson(from), LotusJson(to)| async move {
                    impl_chain_get_path(&ctx.chain_store, &from, &to)
                        .map(LotusJson)
                        .map_err(Into::into)
                },
            )
            .serve(CHAIN_GET_GENESIS, [], |ctx: Ctx<BS>| async move {
                let genesis = ctx.state_manager.chain_store().genesis_block_header();
                Ok(Some(LotusJson(Tipset::from(genesis))))
            })
            .serve(CHAIN_HEAD, [], |ctx: Ctx<BS>| async move {
                let heaviest = ctx.state_manager.chain_store().heaviest_tipset();
                Ok(LotusJson(Tipset::clone(&heaviest)))
            })
            .serve(
                CHAIN_GET_BLOCK,
                ["cid"],
                |ctx: Ctx<BS>, LotusJson(blk_cid)| async move {
                    let blk = ctx
                        .state_manager
                        .blockstore()
                        .get_cbor::<CachingBlockHeader>(&blk_cid)?
                        .context("can't find BlockHeader with that cid")?;
                    Ok(LotusJson(blk))
                },
            )
            .serve(
                CHAIN_GET_TIPSET,
                ["tsk"],
                |ctx: Ctx<BS>, LotusJson(ApiTipsetKey(tsk))| async move {
                    let ts = ctx
                        .state_manager
                        .chain_store()
                        .load_required_tipset_or_heaviest(&tsk)?;
                    Ok(LotusJson(Tipset::clone(&ts)))
                },
            )
            .serve(
                CHAIN_GET_PARENT_RECEIPTS,
                ["block_cid"],
                chain_get_parent_receipts,
            )
            .serve(CHAIN_EXPORT, ["params"], chain_export)
            .serve(
                CHAIN_GET_BLOCK_MESSAGES,
                ["blk_cid"],
                chain_get_block_messages,
            )
            .serve(
                CHAIN_GET_TIPSET_BY_HEIGHT,
                ["height", "tsk"],
                chain_get_tipset_by_height,
            )
            .serve(
                CHAIN_GET_TIPSET_AFTER_HEIGHT,
                ["height", "tsk"],
                chain_get_tipset_after_height,
            )
            .serve(CHAIN_SET_HEAD, ["tsk"], chain_set_head)
            .serve(
                CHAIN_GET_MIN_BASE_FEE,
                ["basefee_lookback"],
                chain_get_min_base_fee,
            )
    };
}

async fn chain_get_parent_receipts(
    ctx: Ctx<impl Blockstore>,
    LotusJson(block_cid): LotusJson<Cid>,
) -> Result<LotusJson<Vec<ApiReceipt>>, JsonRpcError> {
    let store = ctx.state_manager.blockstore();
    let block_header: CachingBlockHeader = store
        .get_cbor(&block_cid)?
        .with_context(|| format!("can't find block header with cid {block_cid}"))?;
    let mut receipts = Vec::new();
    if block_header.epoch == 0 {
        return Ok(LotusJson(vec![]));
    }

    // Try Receipt_v4 first. (Receipt_v4 and Receipt_v3 are identical, use v4 here)
    if let Ok(amt) =
        Amt::<fvm_shared4::receipt::Receipt, _>::load(&block_header.message_receipts, store)
            .map_err(|_| {
                ErrorObjectOwned::owned::<()>(
                    1,
                    format!(
                        "failed to root: ipld: could not find {}",
                        block_header.message_receipts
                    ),
                    None,
                )
            })
    {
        amt.for_each(|_, receipt| {
            receipts.push(ApiReceipt {
                exit_code: receipt.exit_code.into(),
                return_data: receipt.return_data.clone().into(),
                gas_used: receipt.gas_used,
                events_root: receipt.events_root.into(),
            });
            Ok(())
        })?;
    } else {
        // Fallback to Receipt_v2.
        let amt =
            Amt::<fvm_shared2::receipt::Receipt, _>::load(&block_header.message_receipts, store)
                .map_err(|_| {
                    ErrorObjectOwned::owned::<()>(
                        1,
                        format!(
                            "failed to root: ipld: could not find {}",
                            block_header.message_receipts
                        ),
                        None,
                    )
                })?;
        amt.for_each(|_, receipt| {
            receipts.push(ApiReceipt {
                exit_code: receipt.exit_code.into(),
                return_data: receipt.return_data.clone().into(),
                gas_used: receipt.gas_used as _,
                events_root: None.into(),
            });
            Ok(())
        })?;
    }

    Ok(LotusJson(receipts))
}

async fn chain_export(
    ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
    params: ChainExportParams,
) -> Result<Option<String>, JsonRpcError> {
    let ChainExportParams {
        epoch,
        recent_roots,
        output_path,
        tipset_keys: LotusJson(ApiTipsetKey(tsk)),
        skip_checksum,
        dry_run,
    } = params;

    static LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    let _locked = LOCK.try_lock();
    if _locked.is_err() {
        return Err(anyhow::anyhow!("Another chain export job is still in progress").into());
    }

    let chain_finality = ctx.state_manager.chain_config().policy.chain_finality;
    if recent_roots < chain_finality {
        return Err(anyhow::anyhow!(format!(
            "recent-stateroots must be greater than {chain_finality}"
        ))
        .into());
    }

    let head = ctx.chain_store.load_required_tipset_or_heaviest(&tsk)?;
    let start_ts =
        ctx.chain_store
            .chain_index
            .tipset_by_height(epoch, head, ResolveNullTipset::TakeOlder)?;

    match if dry_run {
        crate::chain::export::<Sha256>(
            Arc::clone(&ctx.chain_store.db),
            &start_ts,
            recent_roots,
            VoidAsyncWriter,
            CidHashSet::default(),
            skip_checksum,
        )
        .await
    } else {
        let file = tokio::fs::File::create(&output_path).await?;
        crate::chain::export::<Sha256>(
            Arc::clone(&ctx.chain_store.db),
            &start_ts,
            recent_roots,
            file,
            CidHashSet::default(),
            skip_checksum,
        )
        .await
    } {
        Ok(checksum_opt) => Ok(checksum_opt.map(|hash| hash.encode_hex())),
        Err(e) => Err(anyhow::anyhow!(e).into()),
    }
}

async fn chain_get_block_messages(
    ctx: Ctx<impl Blockstore>,
    LotusJson(blk_cid): LotusJson<Cid>,
) -> Result<BlockMessages, JsonRpcError> {
    let blk: CachingBlockHeader = ctx
        .state_manager
        .blockstore()
        .get_cbor(&blk_cid)?
        .context("can't find block with that cid")?;
    let blk_msgs = &blk.messages;
    let (unsigned_cids, signed_cids) =
        crate::chain::read_msg_cids(ctx.state_manager.blockstore(), blk_msgs)?;
    let (bls_msg, secp_msg) = crate::chain::block_messages_from_cids(
        ctx.state_manager.blockstore(),
        &unsigned_cids,
        &signed_cids,
    )?;
    let cids = unsigned_cids
        .into_iter()
        .chain(signed_cids)
        .collect::<Vec<_>>();

    let ret = BlockMessages {
        bls_msg: bls_msg.into(),
        secp_msg: secp_msg.into(),
        cids: cids.into(),
    };
    Ok(ret)
}

/// Find the path between two tipsets, as a series of [`PathChange`]s.
///
/// ```text
/// 0 - A - B - C - D
///     ^~~~~~~~> apply B, C
///
/// 0 - A - B - C - D
///     <~~~~~~~^ revert C, B
///
///     <~~~~~~~~ revert C, B
/// 0 - A - B  - C
///     |
///      -- B' - C'
///      ~~~~~~~~> then apply B', C'
/// ```
///
/// Exposes errors from the [`Blockstore`], and returns an error if there is no common ancestor.
fn impl_chain_get_path(
    chain_store: &ChainStore<impl Blockstore>,
    from: &TipsetKey,
    to: &TipsetKey,
) -> anyhow::Result<Vec<PathChange>> {
    let mut to_revert = chain_store
        .load_required_tipset_or_heaviest(from)
        .context("couldn't load `from`")?;
    let mut to_apply = chain_store
        .load_required_tipset_or_heaviest(to)
        .context("couldn't load `to`")?;

    let mut all_reverts = vec![];
    let mut all_applies = vec![];

    // This loop is guaranteed to terminate if the blockstore contain no cycles.
    // This is currently computationally infeasible.
    while to_revert != to_apply {
        if to_revert.epoch() > to_apply.epoch() {
            let next = chain_store
                .load_required_tipset_or_heaviest(to_revert.parents())
                .context("couldn't load ancestor of `from`")?;
            all_reverts.push(to_revert);
            to_revert = next;
        } else {
            let next = chain_store
                .load_required_tipset_or_heaviest(to_apply.parents())
                .context("couldn't load ancestor of `to`")?;
            all_applies.push(to_apply);
            to_apply = next;
        }
    }
    Ok(all_reverts
        .into_iter()
        .map(PathChange::Revert)
        .chain(all_applies.into_iter().rev().map(PathChange::Apply))
        .collect())
}

async fn chain_get_tipset_by_height(
    ctx: Ctx<impl Blockstore>,
    height: i64,
    LotusJson(ApiTipsetKey(tsk)): LotusJson<ApiTipsetKey>,
) -> Result<LotusJson<Tipset>, JsonRpcError> {
    let ts = ctx
        .state_manager
        .chain_store()
        .load_required_tipset_or_heaviest(&tsk)?;
    let tss = ctx
        .state_manager
        .chain_store()
        .chain_index
        .tipset_by_height(height, ts, ResolveNullTipset::TakeOlder)?;
    Ok((*tss).clone().into())
}

async fn chain_get_tipset_after_height(
    ctx: Ctx<impl Blockstore>,
    height: i64,
    LotusJson(ApiTipsetKey(tsk)): LotusJson<ApiTipsetKey>,
) -> Result<LotusJson<Tipset>, JsonRpcError> {
    let ts = ctx
        .state_manager
        .chain_store()
        .load_required_tipset_or_heaviest(&tsk)?;
    let tss = ctx
        .state_manager
        .chain_store()
        .chain_index
        .tipset_by_height(height, ts, ResolveNullTipset::TakeNewer)?;
    Ok((*tss).clone().into())
}

// This is basically a port of the reference implementation at
// https://github.com/filecoin-project/lotus/blob/v1.23.0/node/impl/full/chain.go#L321
async fn chain_set_head(
    ctx: Ctx<impl Blockstore>,
    LotusJson(ApiTipsetKey(tsk)): LotusJson<ApiTipsetKey>,
) -> Result<(), JsonRpcError> {
    let new_head = ctx
        .state_manager
        .chain_store()
        .load_required_tipset_or_heaviest(&tsk)?;
    let mut current = ctx.state_manager.chain_store().heaviest_tipset();
    while current.epoch() >= new_head.epoch() {
        for cid in current.key().to_cids() {
            ctx.state_manager
                .chain_store()
                .unmark_block_as_validated(&cid);
        }
        let parents = &current.block_headers().first().parents;
        current = ctx
            .state_manager
            .chain_store()
            .chain_index
            .load_required_tipset(parents)?;
    }
    ctx.state_manager
        .chain_store()
        .set_heaviest_tipset(new_head)
        .map_err(Into::into)
}

async fn chain_get_min_base_fee(
    ctx: Ctx<impl Blockstore>,
    basefee_lookback: u32,
) -> Result<String, JsonRpcError> {
    let mut current = ctx.state_manager.chain_store().heaviest_tipset();
    let mut min_base_fee = current.block_headers().first().parent_base_fee.clone();

    for _ in 0..basefee_lookback {
        let parents = &current.block_headers().first().parents;
        current = ctx
            .state_manager
            .chain_store()
            .chain_index
            .load_required_tipset(parents)?;

        min_base_fee = min_base_fee.min(current.block_headers().first().parent_base_fee.to_owned());
    }

    Ok(min_base_fee.atto().to_string())
}

pub fn chain_notify(ctx: Ctx<impl Blockstore>) -> Subscriber<ApiHeadChange> {
    let (sender, receiver) = broadcast::channel(100);

    // As soon as the channel is created, send the current tipset
    let current = ctx.chain_store.heaviest_tipset();
    let (change, headers) = ("current".into(), current.block_headers().clone().into());
    sender
        .send(ApiHeadChange { change, headers })
        .expect("receiver is not dropped");

    let mut subscriber = ctx.chain_store.publisher().subscribe();

    tokio::spawn(async move {
        while let Ok(v) = subscriber.recv().await {
            let (change, headers) = match v {
                HeadChange::Apply(ts) => ("apply".into(), ts.block_headers().clone().into()),
            };

            if sender.send(ApiHeadChange { change, headers }).is_err() {
                break;
            }
        }
    });
    receiver
}

fn load_api_messages_from_tipset(
    store: &impl Blockstore,
    tipset: &Tipset,
) -> Result<Vec<ApiMessage>, JsonRpcError> {
    let full_tipset = tipset
        .fill_from_blockstore(store)
        .context("Failed to load full tipset")?;
    let blocks = full_tipset.into_blocks();
    let mut messages = vec![];
    let mut seen = CidHashSet::default();
    for block in blocks {
        for msg in block.bls_msgs() {
            let cid = msg.cid()?;
            if seen.insert(cid) {
                messages.push(ApiMessage::new(cid, msg.clone()));
            }
        }

        for msg in block.secp_msgs() {
            let cid = msg.cid()?;
            if seen.insert(cid) {
                messages.push(ApiMessage::new(cid, msg.message.clone()));
            }
        }
    }

    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use PathChange::{Apply, Revert};

    use crate::{
        blocks::{chain4u, Chain4U, RawBlockHeader},
        db::{car::PlainCar, MemoryDB},
        networks::{self, ChainConfig},
    };

    #[test]
    fn revert_to_ancestor_linear() {
        let store = ChainStore::calibnet();
        chain4u! {
            in store.blockstore();
            [_genesis = store.genesis_block_header()]
            -> [a] -> [b] -> [c, d] -> [e]
        };

        // simple
        assert_path_change(&store, b, a, [Revert(&[b])]);

        // from multi-member tipset
        assert_path_change(&store, [c, d], a, [Revert(&[c, d][..]), Revert(&[b])]);

        // to multi-member tipset
        assert_path_change(&store, e, [c, d], [Revert(e)]);

        // over multi-member tipset
        assert_path_change(&store, e, b, [Revert(&[e][..]), Revert(&[c, d])]);
    }

    /// Mirror how lotus handles passing an incomplete `TipsetKey`s.
    /// Tested on lotus `1.23.2`
    #[test]
    fn incomplete_tipsets() {
        let store = ChainStore::calibnet();
        chain4u! {
            in store.blockstore();
            [_genesis = store.genesis_block_header()]
            -> [a, b] -> [c] -> [d, _e] // this pattern 2 -> 1 -> 2 can be found at calibnet epoch 1369126
        };

        // apply to descendant with incomplete `from`
        assert_path_change(
            &store,
            a,
            c,
            [
                Revert(&[a][..]), // revert the incomplete tipset
                Apply(&[a, b]),   // apply the complete one
                Apply(&[c]),      // apply the destination
            ],
        );

        // apply to descendant with incomplete `to`
        assert_path_change(&store, c, d, [Apply(d)]);

        // revert to ancestor with incomplete `from`
        assert_path_change(&store, d, c, [Revert(d)]);

        // revert to ancestor with incomplete `to`
        assert_path_change(
            &store,
            c,
            a,
            [
                Revert(&[c][..]),
                Revert(&[a, b]), // revert the complete tipset
                Apply(&[a]),     // apply the incomplete one
            ],
        );
    }

    #[test]
    fn apply_to_descendant_linear() {
        let store = ChainStore::calibnet();
        chain4u! {
            in store.blockstore();
            [_genesis = store.genesis_block_header()]
            -> [a] -> [b] -> [c, d] -> [e]
        };

        // simple
        assert_path_change(&store, a, b, [Apply(&[b])]);

        // from multi-member tipset
        assert_path_change(&store, [c, d], e, [Apply(e)]);

        // to multi-member tipset
        assert_path_change(&store, b, [c, d], [Apply([c, d])]);

        // over multi-member tipset
        assert_path_change(&store, b, e, [Apply(&[c, d][..]), Apply(&[e])]);
    }

    #[test]
    fn cross_fork_simple() {
        let store = ChainStore::calibnet();
        chain4u! {
            in store.blockstore();
            [_genesis = store.genesis_block_header()]
            -> [a] -> [b1] -> [c1]
        };
        chain4u! {
            from [a] in store.blockstore();
            [b2] -> [c2]
        };

        // same height
        assert_path_change(&store, b1, b2, [Revert(b1), Apply(b2)]);

        // different height
        assert_path_change(&store, b1, c2, [Revert(b1), Apply(b2), Apply(c2)]);

        let _ = (a, c1);
    }

    impl ChainStore<Chain4U<PlainCar<&'static [u8]>>> {
        fn _load(genesis_car: &'static [u8], genesis_cid: Cid) -> Self {
            let db = Arc::new(Chain4U::with_blockstore(
                PlainCar::new(genesis_car).unwrap(),
            ));
            let genesis_block_header = db.get_cbor(&genesis_cid).unwrap().unwrap();
            ChainStore::new(
                db,
                Arc::new(MemoryDB::default()),
                Arc::new(ChainConfig::calibnet()),
                genesis_block_header,
            )
            .unwrap()
        }
        pub fn calibnet() -> Self {
            Self::_load(
                networks::calibnet::DEFAULT_GENESIS,
                *networks::calibnet::GENESIS_CID,
            )
        }
    }

    /// Utility for writing ergonomic tests
    trait MakeTipset {
        fn make_tipset(self) -> Tipset;
    }

    impl MakeTipset for &RawBlockHeader {
        fn make_tipset(self) -> Tipset {
            Tipset::from(CachingBlockHeader::new(self.clone()))
        }
    }

    impl<const N: usize> MakeTipset for [&RawBlockHeader; N] {
        fn make_tipset(self) -> Tipset {
            self.as_slice().make_tipset()
        }
    }

    impl<const N: usize> MakeTipset for &[&RawBlockHeader; N] {
        fn make_tipset(self) -> Tipset {
            self.as_slice().make_tipset()
        }
    }

    impl MakeTipset for &[&RawBlockHeader] {
        fn make_tipset(self) -> Tipset {
            Tipset::new(self.iter().cloned().cloned()).unwrap()
        }
    }

    #[track_caller]
    fn assert_path_change<T: MakeTipset>(
        store: &ChainStore<impl Blockstore>,
        from: impl MakeTipset,
        to: impl MakeTipset,
        expected: impl IntoIterator<Item = PathChange<T>>,
    ) {
        fn print(path_change: &PathChange) {
            let it = match path_change {
                Revert(it) => {
                    print!("Revert(");
                    it
                }
                Apply(it) => {
                    print!(" Apply(");
                    it
                }
            };
            println!(
                "epoch = {}, key.cid = {})",
                it.epoch(),
                it.key().cid().unwrap()
            )
        }

        let actual =
            impl_chain_get_path(store, from.make_tipset().key(), to.make_tipset().key()).unwrap();
        let expected = expected
            .into_iter()
            .map(|change| match change {
                PathChange::Revert(it) => PathChange::Revert(Arc::new(it.make_tipset())),
                PathChange::Apply(it) => PathChange::Apply(Arc::new(it.make_tipset())),
            })
            .collect::<Vec<_>>();
        if expected != actual {
            println!("SUMMARY");
            println!("=======");
            println!("expected:");
            for it in &expected {
                print(it)
            }
            println!();
            println!("actual:");
            for it in &actual {
                print(it)
            }
            println!("=======\n")
        }
        assert_eq!(
            expected, actual,
            "expected change (left) does not match actual change (right)"
        )
    }
}

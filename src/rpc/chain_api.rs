// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use crate::blocks::{CachingBlockHeader, Tipset, TipsetKey};
use crate::chain::index::ResolveNullTipset;
use crate::chain::ChainStore;
use crate::cid_collections::CidHashSet;
use crate::lotus_json::LotusJson;
use crate::message::ChainMessage;
use crate::rpc_api::data_types::{ApiMessage, ApiReceipt};
use crate::rpc_api::{
    chain_api::*,
    data_types::{BlockMessages, RPCState},
};
use crate::shim::clock::ChainEpoch;
use crate::shim::message::Message;
use crate::utils::io::VoidAsyncWriter;
use anyhow::Context as _;
use cid::Cid;
use fil_actors_shared::fvm_ipld_amt::Amtv0 as Amt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use hex::ToHex;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use once_cell::sync::Lazy;
use sha2::Sha256;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn chain_get_message<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((msg_cid,))): Params<LotusJson<(Cid,)>>,
) -> Result<LotusJson<Message>, JsonRpcError> {
    let chain_message: ChainMessage = data
        .state_manager
        .blockstore()
        .get_cbor(&msg_cid)?
        .ok_or_else(|| format!("can't find message with cid {msg_cid}"))?;
    Ok(LotusJson(match chain_message {
        ChainMessage::Signed(m) => m.into_message(),
        ChainMessage::Unsigned(m) => m,
    }))
}

pub async fn chain_get_parent_message<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((block_cid,))): Params<LotusJson<(Cid,)>>,
) -> Result<LotusJson<Vec<ApiMessage>>, JsonRpcError> {
    let store = data.state_manager.blockstore();
    let block_header: CachingBlockHeader = store
        .get_cbor(&block_cid)?
        .ok_or_else(|| format!("can't find block header with cid {block_cid}"))?;
    if block_header.epoch == 0 {
        Ok(LotusJson(vec![]))
    } else {
        let parent_tipset = Tipset::load_required(store, &block_header.parents)?;
        let messages = load_api_messages_from_tipset(store, &parent_tipset)?;
        Ok(LotusJson(messages))
    }
}

pub async fn chain_get_parent_receipts<DB: Blockstore + Send + Sync + 'static>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((block_cid,))): Params<LotusJson<(Cid,)>>,
) -> Result<LotusJson<Vec<ApiReceipt>>, JsonRpcError> {
    let store = data.state_manager.blockstore();
    let block_header: CachingBlockHeader = store
        .get_cbor(&block_cid)?
        .ok_or_else(|| format!("can't find block header with cid {block_cid}"))?;
    let mut receipts = Vec::new();
    if block_header.epoch == 0 {
        return Ok(LotusJson(vec![]));
    }

    // Try Receipt_v4 first. (Receipt_v4 and Receipt_v3 are identical, use v4 here)
    if let Ok(amt) =
        Amt::<fvm_shared4::receipt::Receipt, _>::load(&block_header.message_receipts, store)
            .map_err(|_| JsonRpcError::Full {
                code: 1,
                message: format!(
                    "failed to root: ipld: could not find {}",
                    block_header.message_receipts
                ),
                data: None,
            })
    {
        amt.for_each(|_, receipt| {
            receipts.push(ApiReceipt {
                exit_code: receipt.exit_code.into(),
                return_data: receipt.return_data.clone(),
                gas_used: receipt.gas_used,
                events_root: receipt.events_root,
            });
            Ok(())
        })?;
    } else {
        // Fallback to Receipt_v2.
        let amt =
            Amt::<fvm_shared2::receipt::Receipt, _>::load(&block_header.message_receipts, store)
                .map_err(|_| JsonRpcError::Full {
                    code: 1,
                    message: format!(
                        "failed to root: ipld: could not find {}",
                        block_header.message_receipts
                    ),
                    data: None,
                })?;
        amt.for_each(|_, receipt| {
            receipts.push(ApiReceipt {
                exit_code: receipt.exit_code.into(),
                return_data: receipt.return_data.clone(),
                gas_used: receipt.gas_used as _,
                events_root: None,
            });
            Ok(())
        })?;
    }

    Ok(LotusJson(receipts))
}

pub(crate) async fn chain_get_messages_in_tipset<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((tsk,))): Params<LotusJson<(TipsetKey,)>>,
) -> Result<LotusJson<Vec<ApiMessage>>, JsonRpcError> {
    let store = data.chain_store.blockstore();
    let tipset = Tipset::load_required(store, &tsk)?;
    let messages = load_api_messages_from_tipset(store, &tipset)?;
    Ok(LotusJson(messages))
}

pub async fn chain_export<DB>(
    data: Data<RPCState<DB>>,
    Params(ChainExportParams {
        epoch,
        recent_roots,
        output_path,
        tipset_keys: tsk,
        skip_checksum,
        dry_run,
    }): Params<ChainExportParams>,
) -> Result<Option<String>, JsonRpcError>
where
    DB: Blockstore + Send + Sync + 'static,
{
    static LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    let _locked = LOCK.try_lock();
    if _locked.is_err() {
        return Err(JsonRpcError::Provided {
            code: http::StatusCode::SERVICE_UNAVAILABLE.as_u16() as _,
            message: "Another chain export job is still in progress",
        });
    }

    let chain_finality = data.state_manager.chain_config().policy.chain_finality;
    if recent_roots < chain_finality {
        Err(&format!(
            "recent-stateroots must be greater than {chain_finality}"
        ))?;
    }

    let head = data.chain_store.load_required_tipset(&tsk)?;
    let start_ts =
        data.chain_store
            .chain_index
            .tipset_by_height(epoch, head, ResolveNullTipset::TakeOlder)?;

    match if dry_run {
        crate::chain::export::<Sha256>(
            Arc::clone(&data.chain_store.db),
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
            Arc::clone(&data.chain_store.db),
            &start_ts,
            recent_roots,
            file,
            CidHashSet::default(),
            skip_checksum,
        )
        .await
    } {
        Ok(checksum_opt) => Ok(checksum_opt.map(|hash| hash.encode_hex())),
        Err(e) => Err(JsonRpcError::from(e)),
    }
}

pub async fn chain_read_obj<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((obj_cid,))): Params<LotusJson<(Cid,)>>,
) -> Result<LotusJson<Vec<u8>>, JsonRpcError> {
    let bytes = data
        .state_manager
        .blockstore()
        .get(&obj_cid)?
        .ok_or("can't find object with that cid")?;
    Ok(LotusJson(bytes))
}

pub async fn chain_has_obj<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((obj_cid,))): Params<LotusJson<(Cid,)>>,
) -> Result<bool, JsonRpcError> {
    Ok(data.state_manager.blockstore().get(&obj_cid)?.is_some())
}

pub async fn chain_get_block_messages<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((blk_cid,))): Params<LotusJson<(Cid,)>>,
) -> Result<BlockMessages, JsonRpcError> {
    let blk: CachingBlockHeader = data
        .state_manager
        .blockstore()
        .get_cbor(&blk_cid)?
        .ok_or("can't find block with that cid")?;
    let blk_msgs = &blk.messages;
    let (unsigned_cids, signed_cids) =
        crate::chain::read_msg_cids(data.state_manager.blockstore(), blk_msgs)?;
    let (bls_msg, secp_msg) = crate::chain::block_messages_from_cids(
        data.state_manager.blockstore(),
        &unsigned_cids,
        &signed_cids,
    )?;
    let cids = unsigned_cids
        .into_iter()
        .chain(signed_cids)
        .collect::<Vec<_>>();

    let ret = BlockMessages {
        bls_msg,
        secp_msg,
        cids,
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
pub async fn chain_get_path<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((from, to))): Params<LotusJson<(TipsetKey, TipsetKey)>>,
) -> Result<LotusJson<Vec<PathChange>>, JsonRpcError> {
    impl_chain_get_path(&data.chain_store, &from, &to)
        .map(LotusJson)
        .map_err(Into::into)
}

fn impl_chain_get_path(
    chain_store: &ChainStore<impl Blockstore>,
    from: &TipsetKey,
    to: &TipsetKey,
) -> anyhow::Result<Vec<PathChange>> {
    let mut to_revert = chain_store
        .load_required_tipset(from)
        .context("couldn't load `from`")?;
    let mut to_apply = chain_store
        .load_required_tipset(to)
        .context("couldn't load `to`")?;

    let mut all_reverts = vec![];
    let mut all_applies = vec![];

    // This loop is guaranteed to terminate if the blockstore contain no cycles.
    // This is currently computationally infeasible.
    while to_revert != to_apply {
        if to_revert.epoch() > to_apply.epoch() {
            let next = chain_store
                .load_required_tipset(to_revert.parents())
                .context("couldn't load ancestor of `from`")?;
            all_reverts.push(to_revert);
            to_revert = next;
        } else {
            let next = chain_store
                .load_required_tipset(to_apply.parents())
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

pub async fn chain_get_tipset_by_height<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((height, tsk))): Params<LotusJson<(ChainEpoch, TipsetKey)>>,
) -> Result<LotusJson<Tipset>, JsonRpcError> {
    let ts = data
        .state_manager
        .chain_store()
        .load_required_tipset(&tsk)?;
    let tss = data
        .state_manager
        .chain_store()
        .chain_index
        .tipset_by_height(height, ts, ResolveNullTipset::TakeOlder)?;
    Ok((*tss).clone().into())
}

pub async fn chain_get_genesis<DB: Blockstore>(
    data: Data<RPCState<DB>>,
) -> Result<Option<LotusJson<Tipset>>, JsonRpcError> {
    let genesis = data.state_manager.chain_store().genesis_block_header();
    Ok(Some(Tipset::from(genesis).into()))
}

pub async fn chain_head<DB: Blockstore>(
    data: Data<RPCState<DB>>,
) -> Result<LotusJson<Tipset>, JsonRpcError> {
    let heaviest = data.state_manager.chain_store().heaviest_tipset();
    Ok((*heaviest).clone().into())
}

pub async fn chain_get_block<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((blk_cid,))): Params<LotusJson<(Cid,)>>,
) -> Result<LotusJson<CachingBlockHeader>, JsonRpcError> {
    let blk: CachingBlockHeader = data
        .state_manager
        .blockstore()
        .get_cbor(&blk_cid)?
        .ok_or("can't find BlockHeader with that cid")?;
    Ok(blk.into())
}

pub async fn chain_get_tipset<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((tsk,))): Params<LotusJson<(TipsetKey,)>>,
) -> Result<LotusJson<Tipset>, JsonRpcError> {
    let ts = data
        .state_manager
        .chain_store()
        .load_required_tipset(&tsk)?;
    Ok((*ts).clone().into())
}

// This is basically a port of the reference implementation at
// https://github.com/filecoin-project/lotus/blob/v1.23.0/node/impl/full/chain.go#L321
pub async fn chain_set_head<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((tsk,))): Params<LotusJson<(TipsetKey,)>>,
) -> Result<(), JsonRpcError> {
    let new_head = data
        .state_manager
        .chain_store()
        .load_required_tipset(&tsk)?;
    let mut current = data.state_manager.chain_store().heaviest_tipset();
    while current.epoch() >= new_head.epoch() {
        for cid in current.key().cids.clone() {
            data.state_manager
                .chain_store()
                .unmark_block_as_validated(&cid);
        }
        let parents = &current.block_headers().first().parents;
        current = data
            .state_manager
            .chain_store()
            .load_required_tipset(parents)?;
    }
    data.state_manager
        .chain_store()
        .set_heaviest_tipset(new_head)
        .map_err(Into::into)
}

pub(crate) async fn chain_get_min_base_fee<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params((basefee_lookback,)): Params<(u32,)>,
) -> Result<String, JsonRpcError> {
    let mut current = data.state_manager.chain_store().heaviest_tipset();
    let mut min_base_fee = current.block_headers().first().parent_base_fee.clone();

    for _ in 0..basefee_lookback {
        let parents = &current.block_headers().first().parents;
        current = data
            .state_manager
            .chain_store()
            .load_required_tipset(parents)?;

        min_base_fee = min_base_fee.min(current.block_headers().first().parent_base_fee.to_owned());
    }

    Ok(min_base_fee.atto().to_string())
}

pub(crate) async fn chain_notify<DB: Blockstore>(
    _data: Data<RPCState<DB>>,
) -> Result<(), JsonRpcError> {
    Err(JsonRpcError::METHOD_NOT_FOUND)
}

fn load_api_messages_from_tipset(
    store: &impl Blockstore,
    tipset: &Tipset,
) -> Result<Vec<ApiMessage>, JsonRpcError> {
    let full_tipset = tipset
        .fill_from_blockstore(store)
        .ok_or_else(|| anyhow::anyhow!("Failed to load full tipset"))?;
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
    fn demo_genesis() {
        let store = ChainStore::calibnet();
        assert!(!store.genesis_block_header().parents.cids.is_empty())
    }

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

        // TODO(aatifsyed): how should we handle incomplete `TipsetKey`s from the user?
        // assert_path_change(&store, e, c, [Revert(e)]); // fails
        // assert_path_change(&store, c, a, [Revert(&[c]), Revert(&[b])]); // passes
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
        fn calibnet() -> Self {
            let db = Arc::new(Chain4U::with_blockstore(
                PlainCar::new(networks::calibnet::DEFAULT_GENESIS).unwrap(),
            ));
            let genesis_block_header = db
                .get_cbor(&networks::calibnet::GENESIS_CID)
                .unwrap()
                .unwrap();
            ChainStore::new(
                db,
                Arc::new(MemoryDB::default()),
                Arc::new(ChainConfig::calibnet()),
                genesis_block_header,
            )
            .unwrap()
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

// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::gas::estimate_message_gas;
use crate::lotus_json::{LotusJson, NotNullVec, lotus_json_with_self};
use crate::message::SignedMessage;
use crate::prelude::*;
use crate::rpc::error::ServerError;
use crate::rpc::types::{ApiTipsetKey, MessageSendSpec};
use crate::rpc::{ApiPaths, Ctx, Permission, RpcMethod};
use crate::shim::{
    address::{Address, Protocol},
    message::Message,
    percent::Percent,
};
use ahash::HashSet;
use enumflags2::BitFlags;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ApiMpoolConfig {
    #[schemars(with = "LotusJson<Vec<Address>>")]
    #[serde(with = "crate::lotus_json")]
    pub priority_addrs: Vec<Address>,
    pub size_limit_high: i64,
    pub size_limit_low: i64,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Percent>")]
    pub replace_by_fee_ratio: Percent,
    #[schemars(with = "LotusJson<Duration>")]
    #[serde(with = "crate::lotus_json")]
    pub prune_cooldown: Duration,
    pub gas_limit_overestimation: f64,
}

lotus_json_with_self!(ApiMpoolConfig);

/// Returns a copy of the current mpool config.
pub enum MpoolGetConfig {}
impl RpcMethod<0> for MpoolGetConfig {
    const NAME: &'static str = "Filecoin.MpoolGetConfig";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: &'static str = "Returns a copy of the current mpool config.";

    type Params = ();
    type Ok = ApiMpoolConfig;

    async fn handle(
        ctx: Ctx,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let cfg = ctx.mpool.config();
        Ok(ApiMpoolConfig {
            priority_addrs: cfg.priority_addrs,
            size_limit_high: cfg.size_limit_high,
            size_limit_low: cfg.size_limit_low,
            replace_by_fee_ratio: cfg.replace_by_fee_ratio,
            prune_cooldown: cfg.prune_cooldown,
            gas_limit_overestimation: cfg.gas_limit_overestimation,
        })
    }
}

/// Gets next nonce for the specified sender.
pub enum MpoolGetNonce {}
impl RpcMethod<1> for MpoolGetNonce {
    const NAME: &'static str = "Filecoin.MpoolGetNonce";
    const PARAM_NAMES: [&'static str; 1] = ["address"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: &'static str = "Returns the current nonce for the specified address.";

    type Params = (Address,);
    type Ok = u64;

    async fn handle(
        ctx: Ctx,
        (address,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(ctx.mpool.get_sequence(&address).await?)
    }
}

/// Return `Vec` of pending messages in `mpool`
pub enum MpoolPending {}
impl RpcMethod<1> for MpoolPending {
    const NAME: &'static str = "Filecoin.MpoolPending";
    const PARAM_NAMES: [&'static str; 1] = ["tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: &'static str = "Returns the pending messages for a given tipset.";

    type Params = (ApiTipsetKey,);
    type Ok = NotNullVec<SignedMessage>;

    async fn handle(
        ctx: Ctx,
        (ApiTipsetKey(tipset_key),): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let mut ts = ctx
            .chain_store()
            .load_required_tipset_or_heaviest(&tipset_key)?;

        let (mut pending, mpts) = ctx.mpool.pending();

        // The mpool is already at or past `ts`, so its pending set needs no on-chain merge.
        if mpts.epoch() > ts.epoch() || mpts == ts {
            return Ok(pending.into());
        }

        let mut have_cids: HashSet<_> = pending.iter().map(|m| m.cid()).collect();

        loop {
            // A null round can make `ts.parents()` skip past `mpts.epoch()`, so this may never
            // match, in which case nothing is excluded and `ts` is merged as-is.
            if mpts.epoch() == ts.epoch() {
                if mpts == ts {
                    break;
                }

                // Exclude what the mpool tipset's blocks already include, so only `ts`-specific
                // messages get merged below.
                // <https://github.com/filecoin-project/lotus/blob/27abf0f16a7f2a83305910f3c2a1844764d20b75/node/impl/full/mpool.go#L94>
                let have = ctx.mpool.messages_for_blocks(mpts.block_headers().iter())?;
                have_cids.extend(have.iter().map(|m| m.cid()));
            }

            let msgs = ctx.mpool.messages_for_blocks(ts.block_headers().iter())?;

            for m in msgs {
                if have_cids.insert(m.cid()) {
                    pending.push(m);
                }
            }

            if mpts.epoch() >= ts.epoch() {
                break;
            }

            ts = ctx.chain_index().load_required_tipset(ts.parents())?;
        }
        Ok(pending.into())
    }
}

/// Return `Vec` of pending messages for inclusion in the next block
pub enum MpoolSelect {}
impl RpcMethod<2> for MpoolSelect {
    const NAME: &'static str = "Filecoin.MpoolSelect";
    const PARAM_NAMES: [&'static str; 2] = ["tipsetKey", "ticketQuality"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: &'static str =
        "Returns a list of pending messages for inclusion in the next block.";

    type Params = (ApiTipsetKey, f64);
    type Ok = Vec<SignedMessage>;

    async fn handle(
        ctx: Ctx,
        (ApiTipsetKey(tipset_key), ticket_quality): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx
            .chain_store()
            .load_required_tipset_or_heaviest(&tipset_key)?;
        Ok(ctx.mpool.select_messages(&ts, ticket_quality)?)
    }
}

/// Add `SignedMessage` to `mpool`, return message CID
pub enum MpoolPush {}
impl RpcMethod<1> for MpoolPush {
    const NAME: &'static str = "Filecoin.MpoolPush";
    const PARAM_NAMES: [&'static str; 1] = ["message"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Write;
    const DESCRIPTION: &'static str = "Adds a signed message to the message pool.";

    type Params = (SignedMessage,);
    type Ok = Cid;

    async fn handle(
        ctx: Ctx,
        (message,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let cid = ctx.mpool.push(message).await?;
        Ok(cid)
    }
}

/// Add a batch of `SignedMessage`s to `mpool`, return message CIDs
pub enum MpoolBatchPush {}
impl RpcMethod<1> for MpoolBatchPush {
    const NAME: &'static str = "Filecoin.MpoolBatchPush";
    const PARAM_NAMES: [&'static str; 1] = ["messages"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Write;
    const DESCRIPTION: &'static str = "Adds a set of signed messages to the message pool.";

    type Params = (Vec<SignedMessage>,);
    type Ok = Vec<Cid>;

    async fn handle(
        ctx: Ctx,
        (messages,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let mut cids = vec![];
        for msg in messages {
            cids.push(ctx.mpool.push(msg).await?);
        }
        Ok(cids)
    }
}

/// Add `SignedMessage` from untrusted source to `mpool`, return message CID
pub enum MpoolPushUntrusted {}
impl RpcMethod<1> for MpoolPushUntrusted {
    const NAME: &'static str = "Filecoin.MpoolPushUntrusted";
    const PARAM_NAMES: [&'static str; 1] = ["message"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Write;
    const DESCRIPTION: &'static str =
        "Adds a message to the message pool with verification checks.";

    type Params = (SignedMessage,);
    type Ok = Cid;

    async fn handle(
        ctx: Ctx,
        (message,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        // Lotus implements a few extra sanity checks that we skip. We skip them
        // because those checks aren't used for messages received from peers and
        // therefore aren't safety critical.
        let cid = ctx.mpool.push_untrusted(message).await?;
        Ok(cid)
    }
}

/// Add a batch of `SignedMessage`s to `mpool`, return message CIDs
pub enum MpoolBatchPushUntrusted {}
impl RpcMethod<1> for MpoolBatchPushUntrusted {
    const NAME: &'static str = "Filecoin.MpoolBatchPushUntrusted";
    const PARAM_NAMES: [&'static str; 1] = ["messages"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Write;
    const DESCRIPTION: &'static str =
        "Adds a set of messages to the message pool with additional verification checks.";

    type Params = (Vec<SignedMessage>,);
    type Ok = Vec<Cid>;

    async fn handle(
        ctx: Ctx,
        (messages,): Self::Params,
        ext: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        // Alias of MpoolBatchPush.
        MpoolBatchPush::handle(ctx, (messages,), ext).await
    }
}

/// Sign given `UnsignedMessage` and add it to `mpool`, return `SignedMessage`
pub enum MpoolPushMessage {}
impl RpcMethod<2> for MpoolPushMessage {
    const NAME: &'static str = "Filecoin.MpoolPushMessage";
    const PARAM_NAMES: [&'static str; 2] = ["message", "sendSpec"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Sign;
    const DESCRIPTION: &'static str =
        "Assigns a nonce, signs, and pushes a message to the mempool.";

    type Params = (Message, Option<MessageSendSpec>);
    type Ok = SignedMessage;

    async fn handle(
        ctx: Ctx,
        (message, send_spec): Self::Params,
        extensions: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let from = message.from;

        let heaviest_tipset = ctx.chain_store().heaviest_tipset();
        let key_addr = ctx
            .state_manager
            .resolve_to_deterministic_address(from, &heaviest_tipset)
            .await?;

        if message.sequence != 0 {
            return Err(anyhow::anyhow!(
                "Expected nonce for MpoolPushMessage is 0, and will be calculated for you"
            )
            .into());
        }

        let _sender_guard = ctx.mpool_locker.take_lock(key_addr).await;

        let mut message =
            estimate_message_gas(&ctx, message, send_spec, Default::default()).await?;
        if message.gas_premium > message.gas_fee_cap {
            return Err(anyhow::anyhow!(
                "After estimation, gas premium is greater than gas fee cap"
            )
            .into());
        }

        if from.protocol() == Protocol::ID {
            message.from = key_addr;
        }

        let balance =
            super::wallet::WalletBalance::handle(ctx.clone(), (message.from,), extensions).await?;
        let required_funds = &message.value + &message.gas_fee_cap * message.gas_limit;
        if balance < required_funds {
            return Err(anyhow::anyhow!(
                "mpool push: not enough funds: {balance} < {required_funds}",
            )
            .into());
        }

        let key = crate::key_management::Key::try_from(crate::key_management::try_find(
            &key_addr,
            &ctx.keystore.as_ref().read(),
        )?)?;
        let eth_chain_id = ctx.chain_config().eth_chain_id;

        let smsg = ctx
            .nonce_tracker
            .sign_and_push(&ctx.mpool, message, &key, eth_chain_id)
            .await?;

        Ok(smsg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{Block, CachingBlockHeader, FullTipset, RawBlockHeader, Tipset};
    use crate::chain::ChainStore;
    use crate::chain_sync::TipsetValidator;
    use crate::rpc::RPCState;
    use crate::rpc::test_utils::chain_store;
    use crate::shim::crypto::{SECP_SIG_LEN, Signature};
    use crate::test_utils::dummy_ticket;
    use fvm_ipld_blockstore::Blockstore;

    /// A secp message, unique per `sequence`.
    fn secp_message(sequence: u64) -> SignedMessage {
        SignedMessage::new_unchecked(
            Message {
                from: Address::new_id(100),
                to: Address::new_id(101),
                sequence,
                ..Default::default()
            },
            Signature::new_secp256k1(vec![0; SECP_SIG_LEN]),
        )
    }

    /// A persisted single-block child of `parent` at an explicit `epoch`. A gap over
    /// `parent.epoch() + 1` models a null round. `ticket` distinguishes siblings.
    fn tipset_at(
        db: &impl Blockstore,
        parent: &Tipset,
        epoch: i64,
        ticket: u8,
        bls: &[Message],
        secp: &[SignedMessage],
    ) -> Tipset {
        let fts = FullTipset::new([Block {
            header: CachingBlockHeader::new(RawBlockHeader {
                parents: parent.key().clone(),
                epoch,
                messages: TipsetValidator::compute_msg_root(db, bls, secp).unwrap(),
                ticket: dummy_ticket(ticket),
                ..Default::default()
            }),
            bls_messages: bls.to_vec(),
            secp_messages: secp.to_vec(),
        }])
        .unwrap();
        fts.persist(db).unwrap();
        fts.into_tipset()
    }

    /// A persisted single-block child of `parent`. `ticket` distinguishes siblings.
    fn tipset_on(
        db: &impl Blockstore,
        parent: &Tipset,
        ticket: u8,
        bls: &[Message],
        secp: &[SignedMessage],
    ) -> Tipset {
        tipset_at(db, parent, parent.epoch() + 1, ticket, bls, secp)
    }

    /// An `RPCState` whose message pool sits on `mpool_ts`.
    fn ctx_on(cs: ChainStore, mpool_ts: &Tipset) -> Arc<RPCState> {
        cs.set_heaviest_tipset(mpool_ts.clone()).unwrap();
        let (ctx, _) = RPCState::for_tests(cs).unwrap();
        assert_eq!(
            &ctx.mpool.pending().1,
            mpool_ts,
            "the pool must adopt the heaviest tipset"
        );
        ctx
    }

    async fn pending_at(ctx: Arc<RPCState>, ts: &Tipset) -> Vec<SignedMessage> {
        let NotNullVec(pending) = MpoolPending::handle(
            ctx,
            (ApiTipsetKey(Some(ts.key().clone())),),
            &Default::default(),
        )
        .await
        .unwrap();
        pending
    }

    /// A tipset that forks away from the pool's own tipset must contribute the messages that are
    /// neither pending nor already in the pool's tipset.
    /// <https://github.com/filecoin-project/lotus/blob/27abf0f16a7f2a83305910f3c2a1844764d20b75/node/impl/full/mpool.go#L70>
    #[tokio::test]
    async fn merges_messages_of_a_same_height_fork() {
        let cs = chain_store();
        let genesis = cs.genesis_tipset();
        let shared = secp_message(0);
        let only_in_fork = secp_message(1);

        let mpool_ts = tipset_on(cs.db(), &genesis, 1, &[], std::slice::from_ref(&shared));
        let fork_ts = tipset_on(cs.db(), &genesis, 2, &[], &[shared, only_in_fork.clone()]);

        let ctx = ctx_on(cs, &mpool_ts);
        assert_eq!(pending_at(ctx, &fork_ts).await, vec![only_in_fork]);
    }

    /// Walking back to the pool's own tipset merges the tipsets in between and stops there.
    #[tokio::test]
    async fn walks_back_to_the_mpool_tipset() {
        let cs = chain_store();
        let genesis = cs.genesis_tipset();
        let in_mpool_ts = secp_message(0);
        let in_child = secp_message(1);

        let mpool_ts = tipset_on(
            cs.db(),
            &genesis,
            1,
            &[],
            std::slice::from_ref(&in_mpool_ts),
        );
        let child_ts = tipset_on(cs.db(), &mpool_ts, 2, &[], std::slice::from_ref(&in_child));

        let ctx = ctx_on(cs, &mpool_ts);
        assert_eq!(pending_at(ctx, &child_ts).await, vec![in_child]);
    }

    /// A null round at the pool's height on the requested branch See the null-round note in
    /// `MpoolPending::handle`.
    #[tokio::test]
    async fn merges_across_a_null_round_past_the_mpool_tipset() {
        let cs = chain_store();
        let genesis = cs.genesis_tipset();
        let only_in_ts = secp_message(1);

        // A common ancestor below the pool's height that the walk can descend to and merge.
        let base = tipset_on(cs.db(), &genesis, 5, &[], &[]);
        // Pool sits on a fork at epoch 2.
        let mpool_ts = tipset_on(cs.db(), &base, 1, &[], &[secp_message(0)]);
        // Requested tipset is at epoch 3 but descends straight from `base` (epoch 1): epoch 2 is a
        // null round on its branch, so its parent sits below the pool's height.
        let ts = tipset_at(cs.db(), &base, 3, 3, &[], std::slice::from_ref(&only_in_ts));

        let ctx = ctx_on(cs, &mpool_ts);
        assert_eq!(pending_at(ctx, &ts).await, vec![only_in_ts]);
    }

    /// A tipset at or behind the pool's own needs no merge at all.
    #[tokio::test]
    async fn does_not_merge_at_or_behind_the_mpool_tipset() {
        let cs = chain_store();
        let genesis = cs.genesis_tipset();
        let mpool_ts = tipset_on(cs.db(), &genesis, 1, &[], &[secp_message(0)]);

        let ctx = ctx_on(cs, &mpool_ts);
        // The pool's own tipset, then one behind it: both take the early return.
        assert!(pending_at(ctx.clone(), &mpool_ts).await.is_empty());
        assert!(pending_at(ctx, &genesis).await.is_empty());
    }

    /// A BLS message whose signature the pool never cached is skipped, rather than failing the
    /// whole call.
    /// <https://github.com/filecoin-project/lotus/blob/27abf0f16a7f2a83305910f3c2a1844764d20b75/chain/messagepool/messagepool.go#L1541>
    #[tokio::test]
    async fn skips_bls_messages_with_an_uncached_signature() {
        let cs = chain_store();
        let genesis = cs.genesis_tipset();
        let only_in_fork = secp_message(1);

        let mpool_ts = tipset_on(cs.db(), &genesis, 1, &[], &[]);
        // The fork carries an unrecoverable BLS message alongside a recoverable secp one.
        let fork_ts = tipset_on(
            cs.db(),
            &genesis,
            2,
            // A BLS message whose signature the pool never cached.
            &[Message {
                from: Address::new_id(200),
                to: Address::new_id(201),
                ..Default::default()
            }],
            std::slice::from_ref(&only_in_fork),
        );

        let ctx = ctx_on(cs, &mpool_ts);
        assert_eq!(pending_at(ctx, &fork_ts).await, vec![only_in_fork]);
    }
}

// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::gas::estimate_message_gas;
use crate::lotus_json::NotNullVec;
use crate::message::SignedMessage;
use crate::rpc::error::ServerError;
use crate::rpc::types::{ApiTipsetKey, MessageSendSpec};
use crate::rpc::{ApiPaths, Ctx, Permission, RpcMethod};
use crate::shim::{
    address::{Address, Protocol},
    message::Message,
};
use ahash::{HashSet, HashSetExt as _};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;

/// Gets next nonce for the specified sender.
pub enum MpoolGetNonce {}
impl RpcMethod<1> for MpoolGetNonce {
    const NAME: &'static str = "Filecoin.MpoolGetNonce";
    const PARAM_NAMES: [&'static str; 1] = ["address"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (Address,);
    type Ok = u64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        Ok(ctx.mpool.get_sequence(&address)?)
    }
}

/// Return `Vec` of pending messages in `mpool`
pub enum MpoolPending {}
impl RpcMethod<1> for MpoolPending {
    const NAME: &'static str = "Filecoin.MpoolPending";
    const PARAM_NAMES: [&'static str; 1] = ["tsk"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (ApiTipsetKey,);
    type Ok = NotNullVec<SignedMessage>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (ApiTipsetKey(tsk),): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let mut ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;

        let (mut pending, mpts) = ctx.mpool.pending()?;

        let mut have_cids = HashSet::new();
        for item in pending.iter() {
            have_cids.insert(item.cid());
        }

        if mpts.epoch() > ts.epoch() {
            return Ok(NotNullVec(pending.into_iter().collect()));
        }

        loop {
            if mpts.epoch() == ts.epoch() {
                if mpts == ts {
                    break;
                }

                // mpts has different blocks than ts
                let have = ctx
                    .mpool
                    .as_ref()
                    .messages_for_blocks(ts.block_headers().iter())?;

                for sm in have {
                    have_cids.insert(sm.cid());
                }
            }

            let msgs = ctx
                .mpool
                .as_ref()
                .messages_for_blocks(ts.block_headers().iter())?;

            for m in msgs {
                if have_cids.contains(&m.cid()) {
                    continue;
                }

                have_cids.insert(m.cid());
                pending.push(m);
            }

            if mpts.epoch() >= ts.epoch() {
                break;
            }

            ts = ctx.chain_index().load_required_tipset(ts.parents())?;
        }
        Ok(NotNullVec(pending.into_iter().collect()))
    }
}

/// Return `Vec` of pending messages for inclusion in the next block
pub enum MpoolSelect {}
impl RpcMethod<2> for MpoolSelect {
    const NAME: &'static str = "Filecoin.MpoolSelect";
    const PARAM_NAMES: [&'static str; 2] = ["tipset_key", "ticket_quality"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (ApiTipsetKey, f64);
    type Ok = Vec<SignedMessage>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (ApiTipsetKey(tsk), ticket_quality): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        Ok(ctx.mpool.select_messages(&ts, ticket_quality)?)
    }
}

/// Add `SignedMessage` to `mpool`, return message CID
pub enum MpoolPush {}
impl RpcMethod<1> for MpoolPush {
    const NAME: &'static str = "Filecoin.MpoolPush";
    const PARAM_NAMES: [&'static str; 1] = ["msg"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Write;

    type Params = (SignedMessage,);
    type Ok = Cid;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (msg,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let cid = ctx.mpool.as_ref().push(msg).await?;
        Ok(cid)
    }
}

/// Add a batch of `SignedMessage`s to `mpool`, return message CIDs
pub enum MpoolBatchPush {}
impl RpcMethod<1> for MpoolBatchPush {
    const NAME: &'static str = "Filecoin.MpoolBatchPush";
    const PARAM_NAMES: [&'static str; 1] = ["msgs"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Write;

    type Params = (Vec<SignedMessage>,);
    type Ok = Vec<Cid>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (msgs,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let mut cids = vec![];
        for msg in msgs {
            cids.push(ctx.mpool.as_ref().push(msg).await?);
        }
        Ok(cids)
    }
}

/// Add `SignedMessage` from untrusted source to `mpool`, return message CID
pub enum MpoolPushUntrusted {}
impl RpcMethod<1> for MpoolPushUntrusted {
    const NAME: &'static str = "Filecoin.MpoolPushUntrusted";
    const PARAM_NAMES: [&'static str; 1] = ["msg"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Write;

    type Params = (SignedMessage,);
    type Ok = Cid;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (msg,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        // Lotus implements a few extra sanity checks that we skip. We skip them
        // because those checks aren't used for messages received from peers and
        // therefore aren't safety critical.
        MpoolPush::handle(ctx, (msg,)).await
    }
}

/// Add a batch of `SignedMessage`s to `mpool`, return message CIDs
pub enum MpoolBatchPushUntrusted {}
impl RpcMethod<1> for MpoolBatchPushUntrusted {
    const NAME: &'static str = "Filecoin.MpoolBatchPushUntrusted";
    const PARAM_NAMES: [&'static str; 1] = ["msgs"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Write;

    type Params = (Vec<SignedMessage>,);
    type Ok = Vec<Cid>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (msgs,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        // Alias of MpoolBatchPush.
        MpoolBatchPush::handle(ctx, (msgs,)).await
    }
}

/// Sign given `UnsignedMessage` and add it to `mpool`, return `SignedMessage`
pub enum MpoolPushMessage {}
impl RpcMethod<2> for MpoolPushMessage {
    const NAME: &'static str = "Filecoin.MpoolPushMessage";
    const PARAM_NAMES: [&'static str; 2] = ["usmg", "spec"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Sign;

    type Params = (Message, Option<MessageSendSpec>);
    type Ok = SignedMessage;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (umsg, spec): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let from = umsg.from;

        let mut keystore = ctx.keystore.as_ref().write().await;
        let heaviest_tipset = ctx.chain_store().heaviest_tipset();
        let key_addr = ctx
            .state_manager
            .resolve_to_key_addr(&from, &heaviest_tipset)
            .await?;

        if umsg.sequence != 0 {
            return Err(anyhow::anyhow!(
                "Expected nonce for MpoolPushMessage is 0, and will be calculated for you"
            )
            .into());
        }
        let mut umsg = estimate_message_gas(&ctx, umsg, spec, Default::default()).await?;
        if umsg.gas_premium > umsg.gas_fee_cap {
            return Err(anyhow::anyhow!(
                "After estimation, gas premium is greater than gas fee cap"
            )
            .into());
        }

        if from.protocol() == Protocol::ID {
            umsg.from = key_addr;
        }
        let nonce = ctx.mpool.get_sequence(&from)?;
        umsg.sequence = nonce;
        let key = crate::key_management::Key::try_from(crate::key_management::try_find(
            &key_addr,
            &mut keystore,
        )?)?;
        let sig = crate::key_management::sign(
            *key.key_info.key_type(),
            key.key_info.private_key(),
            umsg.cid().to_bytes().as_slice(),
        )?;

        let smsg = SignedMessage::new_from_parts(umsg, sig)?;

        ctx.mpool.as_ref().push(smsg.clone()).await?;

        Ok(smsg)
    }
}

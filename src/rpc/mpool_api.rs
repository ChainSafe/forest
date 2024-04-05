// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::gas_api::estimate_message_gas;
use super::RPCState;
use crate::lotus_json::LotusJson;
use crate::message::SignedMessage;
use crate::rpc::error::JsonRpcError;
use crate::rpc::types::{ApiTipsetKey, MessageSendSpec};
use crate::rpc::{reflect::SelfDescribingRpcModule, Ctx, RpcMethod, RpcMethodExt as _};
use crate::shim::{
    address::{Address, Protocol},
    message::Message,
};
use ahash::{HashSet, HashSetExt as _};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;

pub fn register_all(
    module: &mut SelfDescribingRpcModule<RPCState<impl Blockstore + Send + Sync + 'static>>,
) {
    MpoolGetNonce::register(module);
    MpoolPending::register(module);
    MpoolPush::register(module);
    MpoolPushMessage::register(module);
}

/// Gets next nonce for the specified sender.
pub enum MpoolGetNonce {}
impl RpcMethod<1> for MpoolGetNonce {
    const NAME: &'static str = "Filecoin.MpoolGetNonce";
    const PARAM_NAMES: [&'static str; 1] = ["address"];
    type Params = (LotusJson<Address>,);
    type Ok = u64;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address,): Self::Params,
    ) -> Result<Self::Ok, JsonRpcError> {
        Ok(ctx.mpool.get_sequence(&address.into_inner())?)
    }
}

/// Return `Vec` of pending messages in `mpool`
pub enum MpoolPending {}
impl RpcMethod<1> for MpoolPending {
    const NAME: &'static str = "Filecoin.MpoolPending";
    const PARAM_NAMES: [&'static str; 1] = ["tsk"];
    type Params = (LotusJson<ApiTipsetKey>,);
    type Ok = LotusJson<Vec<SignedMessage>>;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (LotusJson(ApiTipsetKey(tsk)),): Self::Params,
    ) -> Result<Self::Ok, JsonRpcError> {
        let mut ts = ctx
            .state_manager
            .chain_store()
            .load_required_tipset_or_heaviest(&tsk)?;

        let (mut pending, mpts) = ctx.mpool.pending()?;

        let mut have_cids = HashSet::new();
        for item in pending.iter() {
            have_cids.insert(item.cid()?);
        }

        if mpts.epoch() > ts.epoch() {
            return Ok(pending.into_iter().collect::<Vec<_>>().into());
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
                    have_cids.insert(sm.cid()?);
                }
            }

            let msgs = ctx
                .mpool
                .as_ref()
                .messages_for_blocks(ts.block_headers().iter())?;

            for m in msgs {
                if have_cids.contains(&m.cid()?) {
                    continue;
                }

                have_cids.insert(m.cid()?);
                pending.push(m);
            }

            if mpts.epoch() >= ts.epoch() {
                break;
            }

            ts = ctx
                .state_manager
                .chain_store()
                .chain_index
                .load_required_tipset(ts.parents())?;
        }
        Ok(pending.into_iter().collect::<Vec<_>>().into())
    }
}

/// Add `SignedMessage` to `mpool`, return message CID
pub enum MpoolPush {}
impl RpcMethod<1> for MpoolPush {
    const NAME: &'static str = "Filecoin.MpoolPush";
    const PARAM_NAMES: [&'static str; 1] = ["msg"];
    type Params = (LotusJson<SignedMessage>,);
    type Ok = LotusJson<Cid>;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (LotusJson(msg),): Self::Params,
    ) -> Result<Self::Ok, JsonRpcError> {
        let cid = ctx.mpool.as_ref().push(msg).await?;
        Ok(cid.into())
    }
}

/// Sign given `UnsignedMessage` and add it to `mpool`, return `SignedMessage`
pub enum MpoolPushMessage {}
impl RpcMethod<2> for MpoolPushMessage {
    const NAME: &'static str = "Filecoin.MpoolPushMessage";
    const PARAM_NAMES: [&'static str; 2] = ["usmg", "spec"];
    type Params = (LotusJson<Message>, Option<MessageSendSpec>);
    type Ok = LotusJson<SignedMessage>;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (LotusJson(umsg), spec): Self::Params,
    ) -> Result<Self::Ok, JsonRpcError> {
        let from = umsg.from;

        let mut keystore = ctx.keystore.as_ref().write().await;
        let heaviest_tipset = ctx.state_manager.chain_store().heaviest_tipset();
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
            umsg.cid().unwrap().to_bytes().as_slice(),
        )?;

        let smsg = SignedMessage::new_from_parts(umsg, sig)?;

        ctx.mpool.as_ref().push(smsg.clone()).await?;

        Ok(smsg.into())
    }
}

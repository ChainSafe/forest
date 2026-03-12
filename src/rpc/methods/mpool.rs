// Copyright 2019-2026 ChainSafe Systems
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
use enumflags2::BitFlags;
use fvm_ipld_blockstore::Blockstore;

/// Gets next nonce for the specified sender.
pub enum MpoolGetNonce {}
impl RpcMethod<1> for MpoolGetNonce {
    const NAME: &'static str = "Filecoin.MpoolGetNonce";
    const PARAM_NAMES: [&'static str; 1] = ["address"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the current nonce for the specified address.");

    type Params = (Address,);
    type Ok = u64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(ctx.mpool.get_sequence(&address)?)
    }
}

/// Return `Vec` of pending messages in `mpool`
pub enum MpoolPending {}
impl RpcMethod<1> for MpoolPending {
    const NAME: &'static str = "Filecoin.MpoolPending";
    const PARAM_NAMES: [&'static str; 1] = ["tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the pending messages for a given tipset.");

    type Params = (ApiTipsetKey,);
    type Ok = NotNullVec<SignedMessage>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (ApiTipsetKey(tipset_key),): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let mut ts = ctx
            .chain_store()
            .load_required_tipset_or_heaviest(&tipset_key)?;

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
    const PARAM_NAMES: [&'static str; 2] = ["tipsetKey", "ticketQuality"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns a list of pending messages for inclusion in the next block.");

    type Params = (ApiTipsetKey, f64);
    type Ok = Vec<SignedMessage>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
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
    const DESCRIPTION: Option<&'static str> = Some("Adds a signed message to the message pool.");

    type Params = (SignedMessage,);
    type Ok = Cid;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (message,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let cid = ctx.mpool.as_ref().push(message).await?;
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
    const DESCRIPTION: Option<&'static str> =
        Some("Adds a set of signed messages to the message pool.");

    type Params = (Vec<SignedMessage>,);
    type Ok = Vec<Cid>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (messages,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let mut cids = vec![];
        for msg in messages {
            cids.push(ctx.mpool.as_ref().push(msg).await?);
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
    const DESCRIPTION: Option<&'static str> =
        Some("Adds a message to the message pool with verification checks.");

    type Params = (SignedMessage,);
    type Ok = Cid;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (message,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        // Lotus implements a few extra sanity checks that we skip. We skip them
        // because those checks aren't used for messages received from peers and
        // therefore aren't safety critical.
        let cid = ctx.mpool.as_ref().push_untrusted(message).await?;
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
    const DESCRIPTION: Option<&'static str> =
        Some("Adds a set of messages to the message pool with additional verification checks.");

    type Params = (Vec<SignedMessage>,);
    type Ok = Vec<Cid>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
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
    const DESCRIPTION: Option<&'static str> =
        Some("Assigns a nonce, signs, and pushes a message to the mempool.");

    type Params = (Message, Option<MessageSendSpec>);
    type Ok = SignedMessage;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (message, send_spec): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let from = message.from;

        let heaviest_tipset = ctx.chain_store().heaviest_tipset();
        let key_addr = ctx
            .state_manager
            .resolve_to_key_addr(&from, &heaviest_tipset)
            .await?;

        if message.sequence != 0 {
            return Err(anyhow::anyhow!(
                "Expected nonce for MpoolPushMessage is 0, and will be calculated for you"
            )
            .into());
        }
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
        let nonce = ctx.mpool.get_sequence(&from)?;
        message.sequence = nonce;
        let key = crate::key_management::Key::try_from(crate::key_management::try_find(
            &key_addr,
            &mut ctx.keystore.as_ref().write(),
        )?)?;
        let sig = crate::key_management::sign(
            *key.key_info.key_type(),
            key.key_info.private_key(),
            message.cid().to_bytes().as_slice(),
        )?;

        let smsg = SignedMessage::new_from_parts(message, sig)?;

        ctx.mpool.as_ref().push(smsg.clone()).await?;

        Ok(smsg)
    }
}

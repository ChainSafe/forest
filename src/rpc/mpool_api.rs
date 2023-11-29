// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use std::convert::TryFrom;

use crate::blocks::TipsetKeys;
use crate::lotus_json::LotusJson;
use crate::message::SignedMessage;
use crate::rpc_api::data_types::{MessageSendSpec, RPCState};
use crate::shim::{address::Protocol, message::Message};
use ahash::{HashSet, HashSetExt};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};

use super::gas_api::estimate_message_gas;

/// Return `Vec` of pending messages in `mpool`
pub(in crate::rpc) async fn mpool_pending<DB>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((cid_vec,))): Params<LotusJson<(Vec<Cid>,)>>,
) -> Result<LotusJson<Vec<SignedMessage>>, JsonRpcError>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let tsk = TipsetKeys::from_iter(cid_vec);
    let mut ts = data
        .state_manager
        .chain_store()
        .load_required_tipset(&tsk)?;

    let (mut pending, mpts) = data.mpool.pending()?;

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
            let have = data.mpool.as_ref().messages_for_blocks(ts.blocks())?;

            for sm in have {
                have_cids.insert(sm.cid()?);
            }
        }

        let msgs = data.mpool.as_ref().messages_for_blocks(ts.blocks())?;

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

        ts = data
            .state_manager
            .chain_store()
            .load_required_tipset(ts.parents())?;
    }
    Ok(pending.into_iter().collect::<Vec<_>>().into())
}

/// Add `SignedMessage` to `mpool`, return message CID
pub(in crate::rpc) async fn mpool_push<DB>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((signed_message,))): Params<LotusJson<(SignedMessage,)>>,
) -> Result<LotusJson<Cid>, JsonRpcError>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let cid = data.mpool.as_ref().push(signed_message).await?;

    Ok(cid.into())
}

/// Sign given `UnsignedMessage` and add it to `mpool`, return `SignedMessage`
pub(in crate::rpc) async fn mpool_push_message<DB>(
    data: Data<RPCState<DB>>,
    Params(LotusJson((umsg, spec))): Params<LotusJson<(Message, Option<MessageSendSpec>)>>,
) -> Result<LotusJson<SignedMessage>, JsonRpcError>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let from = umsg.from;

    let mut keystore = data.keystore.as_ref().write().await;
    let heaviest_tipset = data.state_manager.chain_store().heaviest_tipset();
    let key_addr = data
        .state_manager
        .resolve_to_key_addr(&from, &heaviest_tipset)
        .await?;

    if umsg.sequence != 0 {
        return Err(
            "Expected nonce for MpoolPushMessage is 0, and will be calculated for you.".into(),
        );
    }
    let mut umsg = estimate_message_gas::<DB>(&data, umsg, spec, Default::default()).await?;
    if umsg.gas_premium > umsg.gas_fee_cap {
        return Err("After estimation, gas premium is greater than gas fee cap".into());
    }

    if from.protocol() == Protocol::ID {
        umsg.from = key_addr;
    }
    let nonce = data.mpool.get_sequence(&from)?;
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

    data.mpool.as_ref().push(smsg.clone()).await?;

    Ok(smsg.into())
}

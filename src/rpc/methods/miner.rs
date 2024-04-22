// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::beacon::BeaconEntry;
use crate::blocks::{CachingBlockHeader, Ticket, TipsetKey};
use crate::blocks::{ElectionProof, RawBlockHeader};

use crate::chain::{compute_base_fee, ChainStore};

use crate::fil_cns::weight;
use crate::key_management::Key;
use crate::lotus_json::lotus_json_with_self;

use crate::lotus_json::LotusJson;
use crate::message::SignedMessage;
use crate::networks::Height;

use crate::rpc::reflect::Permission;
use crate::rpc::{ApiVersion, Ctx, RpcMethod, ServerError};
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::crypto::{Signature, SignatureType};

use crate::shim::sector::PoStProof;
use crate::utils::db::CborStoreExt;

use anyhow::{Context as _, Result};
use bls_signatures::Serialize as _;
use cid::Cid;
use fil_actors_shared::fvm_ipld_amt::Amtv0 as Amt;
use fvm_ipld_blockstore::Blockstore;

use fvm_shared2::crypto::signature::BLS_SIG_LEN;
use group::prime::PrimeCurveAffine as _;
use itertools::Itertools;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use std::sync::Arc;

macro_rules! for_each_method {
    ($callback:ident) => {
        $callback!(crate::rpc::miner::MinerCreateBlock);
    };
}
pub(crate) use for_each_method;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct BlockTemplate {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub miner: Address,
    #[schemars(with = "LotusJson<TipsetKey>")]
    #[serde(with = "crate::lotus_json")]
    pub parents: TipsetKey,
    #[schemars(with = "LotusJson<Ticket>")]
    #[serde(with = "crate::lotus_json")]
    pub ticket: Ticket,
    #[schemars(with = "LotusJson<ElectionProof>")]
    #[serde(with = "crate::lotus_json")]
    pub eproof: ElectionProof,
    #[schemars(with = "LotusJson<Vec<BeaconEntry>>")]
    #[serde(with = "crate::lotus_json")]
    pub beacon_values: Vec<BeaconEntry>,
    #[schemars(with = "LotusJson<Vec<SignedMessage>>")]
    #[serde(with = "crate::lotus_json")]
    pub messages: Vec<SignedMessage>,
    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub epoch: ChainEpoch,
    pub timestamp: u64,
    #[schemars(with = "LotusJson<Vec<PoStProof>>")]
    #[serde(rename = "WinningPoStProof", with = "crate::lotus_json")]
    pub winning_post_proof: Vec<PoStProof>,
}

lotus_json_with_self!(BlockTemplate);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct BlockMessage {
    #[schemars(with = "LotusJson<CachingBlockHeader>")]
    #[serde(with = "crate::lotus_json")]
    header: CachingBlockHeader,
    #[schemars(with = "LotusJson<Vec<Cid>>")]
    #[serde(with = "crate::lotus_json")]
    bls_messages: Vec<Cid>,
    #[schemars(with = "LotusJson<Vec<Cid>>")]
    #[serde(with = "crate::lotus_json")]
    secpk_messages: Vec<Cid>,
}

lotus_json_with_self!(BlockMessage);

pub enum MinerCreateBlock {}
impl RpcMethod<1> for MinerCreateBlock {
    const NAME: &'static str = "Filecoin.MinerCreateBlock";
    const PARAM_NAMES: [&'static str; 1] = ["block_template"];
    const API_VERSION: ApiVersion = ApiVersion::V0;
    const PERMISSION: Permission = Permission::Write;

    type Params = (BlockTemplate,);
    type Ok = BlockMessage;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_template,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let store = ctx.state_manager.blockstore();
        let parent_tipset = ctx
            .chain_store
            .chain_index
            .load_required_tipset(&block_template.parents)?;

        let lookback_state = ChainStore::get_lookback_tipset_for_round(
            ctx.state_manager.chain_store().chain_index.clone(),
            ctx.state_manager.chain_config().clone(),
            parent_tipset.clone(),
            block_template.epoch,
        )
        .map(|(_, s)| Arc::new(s))?;

        let worker = ctx
            .state_manager
            .get_miner_work_addr(*lookback_state, &block_template.miner)?;

        let parent_weight = weight(store, &parent_tipset)?;
        //let parent_weight = parent_tipset.weight().to_owned();
        let parent_base_fee = compute_base_fee(
            store,
            &parent_tipset,
            ctx.state_manager
                .chain_config()
                .height_infos
                .get(&Height::Smoke)
                .context("Missing Smoke height")?
                .epoch,
        )?;
        let (state, receipts) = ctx.state_manager.tipset_state(&parent_tipset).await?;

        let network_version = ctx.state_manager.get_network_version(block_template.epoch);

        let mut bls_messages = Vec::new();
        let mut secpk_messages = Vec::new();
        let mut bls_msg_cids = Vec::new();
        let mut secpk_msg_cids = Vec::new();
        let mut bls_sigs = Vec::new();

        for msg in block_template.messages {
            match msg.signature().signature_type() {
                SignatureType::Bls => {
                    let cid = ctx
                        .chain_store
                        .blockstore()
                        .put_cbor_default(&msg.message)?;
                    bls_msg_cids.push(cid);
                    bls_sigs.push(msg.signature);
                    bls_messages.push(msg.message);
                }
                SignatureType::Secp256k1 | SignatureType::Delegated => {
                    if msg.signature.is_valid_secpk_sig_type(network_version) {
                        let cid = ctx.chain_store.blockstore().put_cbor_default(&msg)?;
                        secpk_msg_cids.push(cid);
                        secpk_messages.push(msg);
                    } else {
                        Err(anyhow::anyhow!(
                            "unknown sig type: {}",
                            msg.signature.signature_type()
                        ))?;
                    }
                }
            }
        }

        let store = ctx.chain_store.blockstore();
        let mut message_array = Amt::<Cid, _>::new(store);
        for (i, cid) in bls_msg_cids.iter().enumerate() {
            message_array.set(i as u64, *cid)?;
        }
        let bls_msgs_root = message_array.flush()?;
        let mut message_array = Amt::<Cid, _>::new(store);
        for (i, cid) in secpk_msg_cids.iter().enumerate() {
            message_array.set(i as u64, *cid)?;
        }
        let secpk_msgs_root = message_array.flush()?;

        let message_meta_cid = store.put_cbor_default(&MessageMeta {
            bls_messages: bls_msgs_root,
            secpk_messages: secpk_msgs_root,
        })?;

        let signatures: Vec<_> = bls_sigs
            .iter()
            .map(|sig| anyhow::Ok(bls_signatures::Signature::from_bytes(sig.bytes())?))
            .try_collect()?;

        let bls_aggregate = if signatures.is_empty() {
            let sig: bls_signatures::Signature = blstrs::G2Affine::identity().into();
            let mut raw_signature: [u8; BLS_SIG_LEN] = [0; BLS_SIG_LEN];
            sig.write_bytes(&mut raw_signature.as_mut())
                .expect("preallocated");
            Signature::new_bls(raw_signature.to_vec())
        } else {
            let bls_aggregate =
                bls_signatures::aggregate(&signatures).context("failed to aggregate signatures")?;
            Signature {
                sig_type: SignatureType::Bls,
                bytes: bls_aggregate.as_bytes().to_vec(),
            }
        };

        let mut block_header = RawBlockHeader {
            miner_address: block_template.miner,
            ticket: block_template.ticket.into(),
            election_proof: block_template.eproof.into(),
            beacon_entries: block_template.beacon_values,
            winning_post_proof: block_template.winning_post_proof,
            parents: block_template.parents,
            weight: parent_weight,
            epoch: block_template.epoch,
            state_root: state,
            message_receipts: receipts,
            messages: message_meta_cid,
            bls_aggregate: bls_aggregate.into(),
            timestamp: block_template.timestamp,
            signature: None,
            fork_signal: Default::default(),
            parent_base_fee,
        };

        let signing_bytes = block_header.signing_bytes();

        let keystore = &mut *ctx.keystore.write().await;
        let key = match crate::key_management::find_key(&worker, keystore) {
            Ok(key) => key,
            Err(_) => {
                let key_info = crate::key_management::try_find(&worker, keystore)?;
                Key::try_from(key_info)?
            }
        };

        let sig = crate::key_management::sign(
            *key.key_info.key_type(),
            key.key_info.private_key(),
            &signing_bytes,
        )?;

        block_header.signature = sig.into();

        // wallet sign in tests - both nodes should have the same key, no?
        let block_message = BlockMessage {
            header: CachingBlockHeader::from(block_header),
            bls_messages: bls_msg_cids,
            secpk_messages: secpk_msg_cids,
        };

        Ok(block_message)
    }
}

#[derive(fvm_ipld_encoding::tuple::Serialize_tuple)]
struct MessageMeta {
    bls_messages: Cid,
    secpk_messages: Cid,
}

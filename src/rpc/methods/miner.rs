// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::beacon::BeaconEntry;
use crate::blocks::{CachingBlockHeader, Ticket, TipsetKey};
use crate::blocks::{ElectionProof, RawBlockHeader};

use crate::chain::{ChainStore, compute_base_fee};

use crate::fil_cns::weight;
use crate::key_management::{Key, KeyStore};
use crate::lotus_json::lotus_json_with_self;

use crate::lotus_json::LotusJson;
use crate::message::SignedMessage;
use crate::networks::Height;

use crate::rpc::reflect::Permission;
use crate::rpc::types::{ApiTipsetKey, MiningBaseInfo};
use crate::rpc::{ApiPaths, Ctx, RpcMethod, ServerError};
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::crypto::{Signature, SignatureType};
use crate::state_manager::StateLookupPolicy;
use enumflags2::BitFlags;

use crate::shim::sector::PoStProof;
use crate::utils::db::CborStoreExt;

use crate::shim::crypto::BLS_SIG_LEN;
use anyhow::{Context as _, Result};
use bls_signatures::Serialize as _;
use cid::Cid;
use fil_actors_shared::fvm_ipld_amt::Amtv0 as Amt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::tuple::*;
use group::prime::PrimeCurveAffine as _;
use itertools::Itertools;
use parking_lot::RwLock;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use std::sync::Arc;

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

#[derive(Serialize_tuple)]
struct MessageMeta {
    bls_messages: Cid,
    secpk_messages: Cid,
}

pub enum MinerCreateBlock {}
impl RpcMethod<1> for MinerCreateBlock {
    const NAME: &'static str = "Filecoin.MinerCreateBlock";
    const PARAM_NAMES: [&'static str; 1] = ["blockTemplate"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Write;
    const DESCRIPTION: Option<&'static str> = Some(
        "Fills and signs a block template on behalf of the given miner, returning a suitable block header.",
    );

    type Params = (BlockTemplate,);
    type Ok = BlockMessage;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_template,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let store = ctx.store();
        let parent_tipset = ctx
            .chain_index()
            .load_required_tipset(&block_template.parents)?;

        let lookback_state = ChainStore::get_lookback_tipset_for_round(
            ctx.chain_index(),
            ctx.chain_config(),
            &parent_tipset,
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
            ctx.chain_config()
                .height_infos
                .get(&Height::Smoke)
                .context("Missing Smoke height")?
                .epoch,
        )?;
        let (state, receipts) = ctx
            .state_manager
            .tipset_state(&parent_tipset, StateLookupPolicy::Disabled)
            .await?;

        let network_version = ctx.state_manager.get_network_version(block_template.epoch);

        let mut bls_messages = Vec::new();
        let mut secpk_messages = Vec::new();
        let mut bls_msg_cids = Vec::new();
        let mut secpk_msg_cids = Vec::new();
        let mut bls_sigs = Vec::new();

        for msg in block_template.messages {
            match msg.signature().signature_type() {
                SignatureType::Bls => {
                    let cid = ctx.store().put_cbor_default(&msg.message)?;
                    bls_msg_cids.push(cid);
                    bls_sigs.push(msg.signature);
                    bls_messages.push(msg.message);
                }
                SignatureType::Secp256k1 | SignatureType::Delegated => {
                    if msg.signature.is_valid_secpk_sig_type(network_version) {
                        let cid = ctx.store().put_cbor_default(&msg)?;
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

        let store = ctx.store();
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

        let bls_aggregate = aggregate_from_bls_signatures(bls_sigs)?;

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

        block_header.signature = sign_block_header(&block_header, &worker, &ctx.keystore)?.into();

        Ok(BlockMessage {
            header: CachingBlockHeader::from(block_header),
            bls_messages: bls_msg_cids,
            secpk_messages: secpk_msg_cids,
        })
    }
}

fn sign_block_header(
    block_header: &RawBlockHeader,
    worker: &Address,
    keystore: &RwLock<KeyStore>,
) -> Result<Signature> {
    let signing_bytes = block_header.signing_bytes();

    let key = {
        let mut keystore = keystore.write();
        match crate::key_management::find_key(worker, &keystore) {
            Ok(key) => key,
            Err(_) => {
                let key_info = crate::key_management::try_find(worker, &mut keystore)?;
                Key::try_from(key_info)?
            }
        }
    };

    let sig = crate::key_management::sign(
        *key.key_info.key_type(),
        key.key_info.private_key(),
        &signing_bytes,
    )?;
    Ok(sig)
}

fn aggregate_from_bls_signatures(bls_sigs: Vec<Signature>) -> anyhow::Result<Signature> {
    let signatures: Vec<_> = bls_sigs
        .iter()
        .map(|sig| anyhow::Ok(bls_signatures::Signature::from_bytes(sig.bytes())?))
        .try_collect()?;

    if signatures.is_empty() {
        let sig: bls_signatures::Signature = blstrs::G2Affine::identity().into();
        let mut raw_signature: [u8; BLS_SIG_LEN] = [0; BLS_SIG_LEN];
        sig.write_bytes(&mut raw_signature.as_mut())?;
        Ok(Signature::new_bls(raw_signature.to_vec()))
    } else {
        let bls_aggregate =
            bls_signatures::aggregate(&signatures).context("failed to aggregate signatures")?;
        Ok(Signature {
            sig_type: SignatureType::Bls,
            bytes: bls_aggregate.as_bytes().to_vec(),
        })
    }
}

pub enum MinerGetBaseInfo {}
impl RpcMethod<3> for MinerGetBaseInfo {
    const NAME: &'static str = "Filecoin.MinerGetBaseInfo";
    const PARAM_NAMES: [&'static str; 3] = ["minerAddress", "epoch", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Retrieves the Miner Actor at the given address and tipset, returning basic information such as power and mining eligibility.",
    );

    type Params = (Address, i64, ApiTipsetKey);
    type Ok = Option<MiningBaseInfo>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (miner_address, epoch, ApiTipsetKey(tipset_key)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tipset = ctx
            .chain_store()
            .load_required_tipset_or_heaviest(&tipset_key)?;

        Ok(ctx
            .state_manager
            .miner_get_base_info(ctx.beacon(), tipset, miner_address, epoch)
            .await?)
    }
}

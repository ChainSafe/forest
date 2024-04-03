// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use super::gas_api;
use crate::blocks::{Tipset, TipsetKey};
use crate::chain::{index::ResolveNullTipset, ChainStore};
use crate::chain_sync::SyncStage;
use crate::lotus_json::LotusJson;
use crate::lotus_json::{lotus_json_with_self, HasLotusJson};
use crate::message::{ChainMessage, SignedMessage};
use crate::rpc::chain_api::get_parent_receipts;
use crate::rpc::error::JsonRpcError;
use crate::rpc::sync_api::sync_state;
use crate::rpc::types::ApiReceipt;
use crate::rpc::types::RPCSyncState;
use crate::rpc::Ctx;
use crate::shim::address::Address as FilecoinAddress;
use crate::shim::crypto::Signature;
use crate::shim::econ::BLOCK_GAS_LIMIT;
use crate::shim::{clock::ChainEpoch, state_tree::StateTree};
use anyhow::{bail, Context, Result};
use cid::{
    multihash::{self, MultihashDigest},
    Cid,
};
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;
use jsonrpsee::types::Params;
use nonempty::nonempty;
use num_bigint;
use num_traits::Zero as _;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use std::{ops::Add, sync::Arc};

pub const ETH_ACCOUNTS: &str = "Filecoin.EthAccounts";
pub const ETH_BLOCK_NUMBER: &str = "Filecoin.EthBlockNumber";
pub const ETH_CHAIN_ID: &str = "Filecoin.EthChainId";
pub const ETH_GAS_PRICE: &str = "Filecoin.EthGasPrice";
pub const ETH_GET_BALANCE: &str = "Filecoin.EthGetBalance";
pub const ETH_GET_BLOCK_BY_HASH: &str = "Filecoin.EthGetBlockByHash";
pub const ETH_GET_BLOCK_BY_NUMBER: &str = "Filecoin.EthGetBlockByNumber";
pub const ETH_SYNCING: &str = "Filecoin.EthSyncing";

const MASKED_ID_PREFIX: [u8; 12] = [0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

const BLOOM_SIZE: usize = 2048;

const BLOOM_SIZE_IN_BYTES: usize = BLOOM_SIZE / 8;

const FULL_BLOOM: [u8; BLOOM_SIZE_IN_BYTES] = [0xff; BLOOM_SIZE_IN_BYTES];

/// Keccak-256 of an RLP of an empty array
const EMPTY_UNCLES: &str = "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347";

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct GasPriceResult(#[serde(with = "crate::lotus_json::hexify")] pub num_bigint::BigInt);

lotus_json_with_self!(GasPriceResult);

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone)]
pub struct BigInt(#[serde(with = "crate::lotus_json::hexify")] pub num_bigint::BigInt);

lotus_json_with_self!(BigInt);

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone)]
pub struct Nonce(#[serde(with = "crate::lotus_json::hexify_bytes")] pub ethereum_types::H64);

lotus_json_with_self!(Nonce);

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone)]
pub struct Bloom(#[serde(with = "crate::lotus_json::hexify_bytes")] pub ethereum_types::Bloom);

lotus_json_with_self!(Bloom);

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone)]
pub struct Uint64(#[serde(with = "crate::lotus_json::hexify")] pub u64);

lotus_json_with_self!(Uint64);

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone)]
pub struct Bytes(#[serde(with = "crate::lotus_json::hexify_vec_bytes")] pub Vec<u8>);

lotus_json_with_self!(Bytes);

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone)]
pub struct Address(#[serde(with = "crate::lotus_json::hexify_bytes")] pub ethereum_types::Address);

lotus_json_with_self!(Address);

impl Address {
    pub fn to_filecoin_address(&self) -> Result<FilecoinAddress, anyhow::Error> {
        if self.is_masked_id() {
            // This is a masked ID address.
            #[allow(clippy::indexing_slicing)]
            let bytes: [u8; 8] =
                core::array::from_fn(|i| self.0.as_fixed_bytes()[MASKED_ID_PREFIX.len() + i]);
            Ok(FilecoinAddress::new_id(u64::from_be_bytes(bytes)))
        } else {
            // Otherwise, translate the address into an address controlled by the
            // Ethereum Address Manager.
            Ok(FilecoinAddress::new_delegated(
                FilecoinAddress::ETHEREUM_ACCOUNT_MANAGER_ACTOR.id()?,
                self.0.as_bytes(),
            )?)
        }
    }

    fn is_masked_id(&self) -> bool {
        self.0.as_bytes().starts_with(&MASKED_ID_PREFIX)
    }
}

impl FromStr for Address {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Address(
            ethereum_types::Address::from_str(s).map_err(|e| anyhow::anyhow!("{e}"))?,
        ))
    }
}

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone)]
pub struct Hash(pub ethereum_types::H256);

impl Hash {
    // Should ONLY be used for blocks and Filecoin messages. Eth transactions expect a different hashing scheme.
    pub fn to_cid(&self) -> cid::Cid {
        let mh = multihash::Code::Blake2b256.digest(self.0.as_bytes());
        Cid::new_v1(fvm_ipld_encoding::DAG_CBOR, mh)
    }
}

impl FromStr for Hash {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Hash(ethereum_types::H256::from_str(s)?))
    }
}

impl From<Cid> for Hash {
    fn from(cid: Cid) -> Self {
        Hash(ethereum_types::H256::from_slice(
            &cid.hash().digest()[0..32],
        ))
    }
}

lotus_json_with_self!(Hash);

#[derive(Debug, Default, Clone)]
pub enum Predefined {
    Earliest,
    Pending,
    #[default]
    Latest,
}

impl fmt::Display for Predefined {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            Predefined::Earliest => "earliest",
            Predefined::Pending => "pending",
            Predefined::Latest => "latest",
        };
        write!(f, "{}", s)
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum BlockNumberOrHash {
    PredefinedBlock(Predefined),
    BlockNumber(i64),
    BlockHash(Hash, bool),
}

impl BlockNumberOrHash {
    pub fn from_predefined(predefined: Predefined) -> Self {
        Self::PredefinedBlock(predefined)
    }

    pub fn from_block_number(number: i64) -> Self {
        Self::BlockNumber(number)
    }
}

impl HasLotusJson for BlockNumberOrHash {
    type LotusJson = String;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        match self {
            Self::PredefinedBlock(predefined) => predefined.to_string(),
            Self::BlockNumber(number) => format!("{:#x}", number),
            Self::BlockHash(hash, _require_canonical) => format!("{:#x}", hash.0),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        match lotus_json.as_str() {
            "earliest" => return Self::PredefinedBlock(Predefined::Earliest),
            "pending" => return Self::PredefinedBlock(Predefined::Pending),
            "latest" => return Self::PredefinedBlock(Predefined::Latest),
            _ => (),
        };

        #[allow(clippy::indexing_slicing)]
        if lotus_json.len() > 2 && &lotus_json[..2] == "0x" {
            if let Ok(number) = i64::from_str_radix(&lotus_json[2..], 16) {
                return Self::BlockNumber(number);
            }
        }

        // Return some default value if we can't convert
        Self::PredefinedBlock(Predefined::Latest)
    }
}

#[derive(PartialEq, Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub hash: Hash,
    pub parent_hash: Hash,
    pub sha3_uncles: Hash,
    pub miner: Address,
    pub state_root: Hash,
    pub transactions_root: Hash,
    pub receipts_root: Hash,
    pub logs_bloom: Bloom,
    pub difficulty: Uint64,
    pub total_difficulty: Uint64,
    pub number: Uint64,
    pub gas_limit: Uint64,
    pub gas_used: Uint64,
    pub timestamp: Uint64,
    pub extra_data: Bytes,
    pub mix_hash: Hash,
    pub nonce: Nonce,
    pub base_fee_per_gas: BigInt,
    pub size: Uint64,
    // can be Vec<Tx> or Vec<String> depending on query params
    pub transactions: Vec<Tx>,
    pub uncles: Vec<Hash>,
}

impl Block {
    pub fn new() -> Self {
        Self {
            gas_limit: Uint64(BLOCK_GAS_LIMIT),
            logs_bloom: Bloom(ethereum_types::Bloom(FULL_BLOOM)),
            sha3_uncles: Hash(ethereum_types::H256::from_str(EMPTY_UNCLES).unwrap()),
            ..Default::default()
        }
    }
}

lotus_json_with_self!(Block);

#[derive(PartialEq, Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tx {
    pub chain_id: u64,
    pub nonce: u64,
    pub hash: Hash,
    pub block_hash: Hash,
    pub block_number: Uint64,
    pub transaction_index: Uint64,
    pub from: Address,
    pub to: Address,
    pub value: BigInt,
    pub r#type: u64,
    pub input: Vec<u8>,
    pub gas: u64,
    pub max_fee_per_gas: BigInt,
    pub max_priority_fee_per_gas: BigInt,
    pub access_list: Vec<Hash>,
    pub v: BigInt,
    pub r: BigInt,
    pub s: BigInt,
}

lotus_json_with_self!(Tx);

#[derive(Debug, Clone, Default)]
pub struct EthSyncingResult {
    pub done_sync: bool,
    pub starting_block: i64,
    pub current_block: i64,
    pub highest_block: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EthSyncingResultLotusJson {
    DoneSync(bool),
    Syncing {
        #[serde(rename = "startingblock", with = "crate::lotus_json::hexify")]
        starting_block: i64,
        #[serde(rename = "currentblock", with = "crate::lotus_json::hexify")]
        current_block: i64,
        #[serde(rename = "highestblock", with = "crate::lotus_json::hexify")]
        highest_block: i64,
    },
}

impl HasLotusJson for EthSyncingResult {
    type LotusJson = EthSyncingResultLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        match self {
            Self {
                done_sync: false,
                starting_block,
                current_block,
                highest_block,
            } => EthSyncingResultLotusJson::Syncing {
                starting_block,
                current_block,
                highest_block,
            },
            _ => EthSyncingResultLotusJson::DoneSync(false),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        match lotus_json {
            EthSyncingResultLotusJson::DoneSync(syncing) => {
                if syncing {
                    // Dangerous to panic here, log error instead.
                    tracing::error!("Invalid EthSyncingResultLotusJson: {syncing}");
                }
                Self {
                    done_sync: true,
                    ..Default::default()
                }
            }
            EthSyncingResultLotusJson::Syncing {
                starting_block,
                current_block,
                highest_block,
            } => Self {
                done_sync: false,
                starting_block,
                current_block,
                highest_block,
            },
        }
    }
}

pub async fn eth_accounts() -> Result<Vec<String>, JsonRpcError> {
    // EthAccounts will always return [] since we don't expect Forest to manage private keys
    Ok(vec![])
}

pub async fn eth_block_number<DB: Blockstore>(data: Ctx<DB>) -> Result<String, JsonRpcError> {
    // `eth_block_number` needs to return the height of the latest committed tipset.
    // Ethereum clients expect all transactions included in this block to have execution outputs.
    // This is the parent of the head tipset. The head tipset is speculative, has not been
    // recognized by the network, and its messages are only included, not executed.
    // See https://github.com/filecoin-project/ref-fvm/issues/1135.
    let heaviest = data.state_manager.chain_store().heaviest_tipset();
    if heaviest.epoch() == 0 {
        // We're at genesis.
        return Ok("0x0".to_string());
    }
    // First non-null parent.
    let effective_parent = heaviest.parents();
    if let Ok(Some(parent)) = data
        .state_manager
        .chain_store()
        .chain_index
        .load_tipset(effective_parent)
    {
        Ok(format!("{:#x}", parent.epoch()))
    } else {
        Ok("0x0".to_string())
    }
}

pub async fn eth_chain_id<DB: Blockstore>(data: Ctx<DB>) -> Result<String, JsonRpcError> {
    Ok(format!(
        "{:#x}",
        data.state_manager.chain_config().eth_chain_id
    ))
}

pub async fn eth_gas_price<DB: Blockstore>(data: Ctx<DB>) -> Result<GasPriceResult, JsonRpcError> {
    let ts = data.state_manager.chain_store().heaviest_tipset();
    let block0 = ts.block_headers().first();
    let base_fee = &block0.parent_base_fee;
    if let Ok(premium) = gas_api::estimate_gas_premium(&data, 10000).await {
        let gas_price = base_fee.add(premium);
        Ok(GasPriceResult(gas_price.atto().clone()))
    } else {
        Ok(GasPriceResult(num_bigint::BigInt::zero()))
    }
}

pub async fn eth_get_balance<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<BigInt, JsonRpcError> {
    let LotusJson((address, block_param)): LotusJson<(Address, BlockNumberOrHash)> =
        params.parse()?;

    let fil_addr = address.to_filecoin_address()?;

    let ts = tipset_by_block_number_or_hash(&data.chain_store, block_param)?;

    let state = StateTree::new_from_root(data.state_manager.blockstore_owned(), ts.parent_state())?;

    let actor = state
        .get_actor(&fil_addr)?
        .context("Failed to retrieve actor")?;

    Ok(BigInt(actor.balance.atto().clone()))
}

pub async fn eth_syncing<DB: Blockstore>(
    _params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<EthSyncingResult>, JsonRpcError> {
    let RPCSyncState { active_syncs } = sync_state(data).await?;
    match active_syncs
        .iter()
        .rev()
        .find_or_first(|ss| ss.stage() != SyncStage::Idle)
    {
        Some(sync_state) => match (sync_state.base(), sync_state.target()) {
            (Some(base), Some(target)) => Ok(LotusJson(EthSyncingResult {
                done_sync: sync_state.stage() == SyncStage::Complete,
                current_block: sync_state.epoch(),
                starting_block: base.epoch(),
                highest_block: target.epoch(),
            })),
            _ => Err(JsonRpcError::internal_error(
                "missing syncing information, try again",
                None,
            )),
        },
        None => Err(JsonRpcError::internal_error("sync state not found", None)),
    }
}

fn tipset_by_block_number_or_hash<DB: Blockstore>(
    chain: &Arc<ChainStore<DB>>,
    block_param: BlockNumberOrHash,
) -> anyhow::Result<Arc<Tipset>> {
    let head = chain.heaviest_tipset();

    match block_param {
        BlockNumberOrHash::PredefinedBlock(predefined) => match predefined {
            Predefined::Earliest => bail!("block param \"earliest\" is not supported"),
            Predefined::Pending => Ok(head),
            Predefined::Latest => {
                let parent = chain.chain_index.load_required_tipset(head.parents())?;
                Ok(parent)
            }
        },
        BlockNumberOrHash::BlockNumber(number) => {
            let height = ChainEpoch::from(number);
            if height > head.epoch() - 1 {
                bail!("requested a future epoch (beyond \"latest\")");
            }
            let ts =
                chain
                    .chain_index
                    .tipset_by_height(height, head, ResolveNullTipset::TakeOlder)?;
            Ok(ts)
        }
        BlockNumberOrHash::BlockHash(hash, require_canonical) => {
            let tsk = TipsetKey::from(nonempty![hash.to_cid()]);
            let ts = chain.chain_index.load_required_tipset(&tsk)?;
            // verify that the tipset is in the canonical chain
            if require_canonical {
                // walk up the current chain (our head) until we reach ts.epoch()
                let walk_ts = chain.chain_index.tipset_by_height(
                    ts.epoch(),
                    head,
                    ResolveNullTipset::TakeOlder,
                )?;
                // verify that it equals the expected tipset
                if walk_ts != ts {
                    bail!("tipset is not canonical");
                }
            }
            Ok(ts)
        }
    }
}

pub async fn eth_get_block_by_hash<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<Block, JsonRpcError> {
    todo!()
}

async fn execute_tipset<DB: Blockstore + Send + Sync + 'static>(
    data: Ctx<DB>,
    tipset: &Arc<Tipset>,
) -> Result<(Cid, Vec<ChainMessage>, Vec<ApiReceipt>)> {
    let msgs = data.chain_store.messages_for_tipset(&tipset)?;

    let (state_root, receipt_root) = data.state_manager.tipset_state(tipset).await?;

    let receipts = get_parent_receipts(data, receipt_root).await?;

    if msgs.len() != receipts.len() {
        bail!(
            "receipts and message array lengths didn't match for tipset: {:?}",
            tipset
        )
    }

    Ok((state_root, msgs, receipts))
}

pub fn tx_from_signed_message<S>(smsg: SignedMessage, state: &StateTree<S>) -> Result<Tx> {
    let mut tx: Tx = Tx::default();

    if smsg.is_delegated() {
    } else if smsg.is_secp256k1() {
        // Secp Filecoin Message
        tx.hash = smsg.cid()?.into();
    } else {
        // BLS Filecoin message
        tx.hash = smsg.message().cid()?.into();
    }
    Ok(tx)
}

pub async fn block_from_filecoin_tipset<DB: Blockstore + Send + Sync + 'static>(
    data: Ctx<DB>,
    tipset: Arc<Tipset>,
    full_tx_info: bool,
) -> Result<Block> {
    let parent_cid = tipset.parents().cid()?;

    let block_number = Uint64(tipset.epoch() as u64);

    let tsk = tipset.key();
    let block_cid = tsk.cid()?;
    let block_hash: Hash = block_cid.into();

    let (state_root, msgs, receipts) = execute_tipset(data.clone(), &tipset).await?;

    let state_tree = StateTree::new_from_root(data.state_manager.blockstore_owned(), &state_root)?;

    let mut transactions = vec![];
    let mut gas_used = 0;
    for (i, msg) in msgs.iter().enumerate() {
        let receipt = receipts[i].clone();
        let ti = Uint64(i as u64);
        gas_used += receipt.gas_used;
        let smsg = match msg {
            ChainMessage::Signed(msg) => msg.clone(),
            ChainMessage::Unsigned(msg) => {
                let sig = Signature::new_bls(vec![]);
                SignedMessage::new_unchecked(msg.clone(), sig)
            }
        };

        // TODO: build tx and push to block transactions
        let mut tx = tx_from_signed_message(smsg, &state_tree)?;
        tx.block_hash = block_hash.clone();
        tx.block_number = block_number.clone();
        tx.transaction_index = ti;

        if full_tx_info {
            transactions.push(tx);
        } else {
            // TODO: push in some other vector
        }
    }

    let mut block = Block::new();
    block.hash = block_hash;
    block.number = block_number;
    block.parent_hash = parent_cid.into();
    block.timestamp = Uint64(tipset.block_headers().first().timestamp);
    block.base_fee_per_gas = BigInt(
        tipset
            .block_headers()
            .first()
            .parent_base_fee
            .atto()
            .clone(),
    );
    block.gas_used = Uint64(gas_used);
    block.transactions = transactions;

    Ok(block)
}

pub async fn eth_get_block_by_number<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<Block, JsonRpcError> {
    let LotusJson((block_param, full_tx_info)): LotusJson<(BlockNumberOrHash, bool)> =
        params.parse()?;

    dbg!(&block_param);
    dbg!(&full_tx_info);

    let ts = tipset_by_block_number_or_hash(&data.chain_store, block_param)?;

    let block = block_from_filecoin_tipset(data, ts, full_tx_info).await?;

    Ok(block)
}

#[cfg(test)]
mod test {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn gas_price_result_serde_roundtrip(i: u128) {
        let r = GasPriceResult(i.into());
        let encoded = serde_json::to_string(&r).unwrap();
        assert_eq!(encoded, format!("\"{i:#x}\""));
        let decoded: GasPriceResult = serde_json::from_str(&encoded).unwrap();
        assert_eq!(r.0, decoded.0);
    }
}

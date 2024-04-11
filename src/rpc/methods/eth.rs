// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use super::gas;
use crate::blocks::{Tipset, TipsetKey};
use crate::chain::{index::ResolveNullTipset, ChainStore};
use crate::chain_sync::SyncStage;
use crate::lotus_json::LotusJson;
use crate::lotus_json::{lotus_json_with_self, HasLotusJson};
use crate::message::{ChainMessage, Message as _, SignedMessage};
use crate::rpc::chain::{get_parent_receipts, ApiReceipt};
use crate::rpc::error::ServerError;
use crate::rpc::sync::sync_state;
use crate::rpc::types::RPCSyncState;
use crate::rpc::Ctx;
use crate::shim::address::{Address as FilecoinAddress, Protocol};
use crate::shim::crypto::{Signature, SignatureType};
use crate::shim::econ::{TokenAmount, BLOCK_GAS_LIMIT};
use crate::shim::fvm_shared_latest::address::{Address as VmAddress, DelegatedAddress};
use crate::shim::fvm_shared_latest::MethodNum;
use crate::shim::fvm_shared_latest::METHOD_CONSTRUCTOR;
use crate::shim::message::Message;
use crate::shim::{clock::ChainEpoch, state_tree::StateTree};

use anyhow::{bail, Context, Result};
use bytes::{Buf, BytesMut};
use cbor4ii::core::{dec::Decode, utils::SliceReader, Value};
use cid::{
    multihash::{self, MultihashDigest},
    Cid,
};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{CBOR, DAG_CBOR, IPLD_RAW};
use itertools::Itertools;
use jsonrpsee::types::Params;
use keccak_hash::keccak;
use nonempty::nonempty;
use num_bigint;
use num_bigint::Sign;
use num_traits::Zero as _;
use rlp::RlpStream;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use std::{ops::Add, sync::Arc};

pub const ETH_ACCOUNTS: &str = "Filecoin.EthAccounts";
pub const ETH_BLOCK_NUMBER: &str = "Filecoin.EthBlockNumber";
pub const ETH_CHAIN_ID: &str = "Filecoin.EthChainId";
pub const ETH_GAS_PRICE: &str = "Filecoin.EthGasPrice";
pub const ETH_GET_BALANCE: &str = "Filecoin.EthGetBalance";
pub const ETH_GET_BLOCK_BY_NUMBER: &str = "Filecoin.EthGetBlockByNumber";
pub const ETH_SYNCING: &str = "Filecoin.EthSyncing";
pub const WEB3_CLIENT_VERSION: &str = "Filecoin.Web3ClientVersion";

const MASKED_ID_PREFIX: [u8; 12] = [0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

const BLOOM_SIZE: usize = 2048;

const BLOOM_SIZE_IN_BYTES: usize = BLOOM_SIZE / 8;

const FULL_BLOOM: [u8; BLOOM_SIZE_IN_BYTES] = [0xff; BLOOM_SIZE_IN_BYTES];

const ADDRESS_LENGTH: usize = 20;

/// Keccak-256 of an RLP of an empty array
const EMPTY_UNCLES: &str = "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347";

const EIP_1559_TX_TYPE: u64 = 2;

/// The address used in messages to actors that have since been deleted.
const REVERTED_ETH_ADDRESS: &str = "0xff0000000000000000000000ffffffffffffffff";

#[repr(u64)]
enum EAMMethod {
    Constructor = METHOD_CONSTRUCTOR,
    Create = 2,
    Create2 = 3,
    CreateExternal = 4,
}

#[repr(u64)]
enum EVMMethod {
    Constructor = METHOD_CONSTRUCTOR,
    Resurrect = 2,
    GetBytecode = 3,
    GetBytecodeHash = 4,
    GetStorageAt = 5,
    InvokeContractDelegate = 6,
    // it is very unfortunate but the hasher creates a circular dependency, so we use the raw
    // number.
    // InvokeContract = frc42_dispatch::method_hash!("InvokeEVM"),
    InvokeContract = 3844450837,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct GasPriceResult(#[serde(with = "crate::lotus_json::hexify")] pub num_bigint::BigInt);

lotus_json_with_self!(GasPriceResult);

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone)]
pub struct BigInt(#[serde(with = "crate::lotus_json::hexify")] pub num_bigint::BigInt);

impl From<TokenAmount> for BigInt {
    fn from(amount: TokenAmount) -> Self {
        Self(amount.atto().to_owned())
    }
}

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

    pub fn from_filecoin_address(addr: &FilecoinAddress) -> Result<Self> {
        match addr.protocol() {
            Protocol::ID => Ok(Self::from_actor_id(addr.id()?)),
            Protocol::Delegated => {
                let payload = addr.payload();
                let result: Result<DelegatedAddress, _> = payload.try_into();
                if let Ok(f4_addr) = result {
                    let namespace = f4_addr.namespace();
                    if namespace != FilecoinAddress::ETHEREUM_ACCOUNT_MANAGER_ACTOR.id()? {
                        bail!("invalid address {addr}");
                    }
                    let eth_addr = cast_eth_addr(f4_addr.subaddress())?;
                    if eth_addr.is_masked_id() {
                        bail!(
                            "f410f addresses cannot embed masked-ID payloads: {}",
                            eth_addr.0
                        );
                    }
                    Ok(eth_addr)
                } else {
                    bail!("invalid delegated address namespace in: {addr}")
                }
            }
            _ => {
                bail!("invalid address {addr}");
            }
        }
    }

    fn is_masked_id(&self) -> bool {
        self.0.as_bytes().starts_with(&MASKED_ID_PREFIX)
    }

    fn from_actor_id(id: u64) -> Self {
        let mut payload = ethereum_types::H160::default();
        payload.as_bytes_mut()[0] = 0xff;
        payload.as_bytes_mut()[12..20].copy_from_slice(&id.to_be_bytes());

        Self(payload)
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

fn cast_eth_addr(bytes: &[u8]) -> Result<Address> {
    if bytes.len() != ADDRESS_LENGTH {
        bail!("cannot parse bytes into an Ethereum address: incorrect input length")
    }
    let mut payload = ethereum_types::H160::default();
    payload.as_bytes_mut().copy_from_slice(bytes);
    Ok(Address(payload))
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
        let (_, digest, _) = cid.hash().into_inner();
        Hash(ethereum_types::H256::from_slice(&digest[0..32]))
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:#x}", self.0)
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

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)] // try a Vec<String>, then a Vec<Tx>
pub enum Transactions {
    Hash(Vec<String>),
    Full(Vec<Tx>),
}

impl Default for Transactions {
    fn default() -> Self {
        Self::Hash(vec![])
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
    pub transactions: Transactions,
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
    pub chain_id: Uint64,
    pub nonce: Uint64,
    pub hash: Hash,
    pub block_hash: Hash,
    pub block_number: Uint64,
    pub transaction_index: Uint64,
    pub from: Address,
    pub to: Address,
    pub value: BigInt,
    pub r#type: Uint64,
    pub input: Bytes,
    pub gas: Uint64,
    pub max_fee_per_gas: BigInt,
    pub max_priority_fee_per_gas: BigInt,
    pub access_list: Vec<Hash>,
    pub v: BigInt,
    pub r: BigInt,
    pub s: BigInt,
}

impl Tx {
    pub fn eth_hash(&self) -> Hash {
        let eth_tx_args: TxArgs = self.clone().into();
        eth_tx_args.hash()
    }
}

lotus_json_with_self!(Tx);

#[derive(Debug, Clone, Default)]
struct TxArgs {
    pub chain_id: u64,
    pub nonce: u64,
    pub to: Address,
    pub value: BigInt,
    pub max_fee_per_gas: BigInt,
    pub max_priority_fee_per_gas: BigInt,
    pub gas_limit: u64,
    pub input: Vec<u8>,
    pub v: BigInt,
    pub r: BigInt,
    pub s: BigInt,
}

impl From<Tx> for TxArgs {
    fn from(tx: Tx) -> Self {
        Self {
            chain_id: tx.chain_id.0,
            nonce: tx.nonce.0,
            to: tx.to,
            value: tx.value,
            max_fee_per_gas: tx.max_fee_per_gas,
            max_priority_fee_per_gas: tx.max_priority_fee_per_gas,
            gas_limit: tx.gas.0,
            input: tx.input.0,
            v: tx.v,
            r: tx.r,
            s: tx.s,
        }
    }
}

fn format_u64(value: &u64) -> BytesMut {
    let bytes = value.to_be_bytes();
    let first_non_zero = bytes.iter().position(|&b| b != 0);

    match first_non_zero {
        Some(i) => bytes[i..].into(),
        None => {
            // If all bytes are zero, return an empty slice
            BytesMut::new()
        }
    }
}

fn format_bigint(value: &BigInt) -> BytesMut {
    let (_, bytes) = value.0.to_bytes_be();
    let first_non_zero = bytes.iter().position(|&b| b != 0);

    match first_non_zero {
        Some(i) => bytes[i..].into(),
        None => {
            // If all bytes are zero, return an empty slice
            BytesMut::new()
        }
    }
}

fn format_address(value: &Address) -> BytesMut {
    value.0.as_bytes().into()
}

impl TxArgs {
    pub fn hash(&self) -> Hash {
        Hash(keccak(self.rlp_signed_message()))
    }

    pub fn rlp_signed_message(&self) -> Vec<u8> {
        let mut stream = RlpStream::new_list(12); // THIS IS IMPORTANT
        stream.append(&format_u64(&self.chain_id));
        stream.append(&format_u64(&self.nonce));
        stream.append(&format_bigint(&self.max_priority_fee_per_gas));
        stream.append(&format_bigint(&self.max_fee_per_gas));
        stream.append(&format_u64(&self.gas_limit));
        stream.append(&format_address(&self.to));
        stream.append(&format_bigint(&self.value));
        stream.append(&self.input);
        let access_list: &[u8] = &[];
        stream.append_list(access_list);

        stream.append(&format_bigint(&self.v));
        stream.append(&format_bigint(&self.r));
        stream.append(&format_bigint(&self.s));

        let mut rlp = stream.out()[..].to_vec();
        let mut bytes: Vec<u8> = vec![0x02];
        bytes.append(&mut rlp);

        let hex = bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join("");
        tracing::trace!("rlp: {}", &hex);

        bytes
    }
}

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

pub async fn eth_accounts() -> Result<Vec<String>, ServerError> {
    // EthAccounts will always return [] since we don't expect Forest to manage private keys
    Ok(vec![])
}

pub async fn eth_block_number<DB: Blockstore>(data: Ctx<DB>) -> Result<String, ServerError> {
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

pub async fn eth_chain_id<DB: Blockstore>(data: Ctx<DB>) -> Result<String, ServerError> {
    Ok(format!(
        "{:#x}",
        data.state_manager.chain_config().eth_chain_id
    ))
}

pub async fn eth_gas_price<DB: Blockstore>(data: Ctx<DB>) -> Result<GasPriceResult, ServerError> {
    let ts = data.state_manager.chain_store().heaviest_tipset();
    let block0 = ts.block_headers().first();
    let base_fee = &block0.parent_base_fee;
    if let Ok(premium) = gas::estimate_gas_premium(&data, 10000).await {
        let gas_price = base_fee.add(premium);
        Ok(GasPriceResult(gas_price.atto().clone()))
    } else {
        Ok(GasPriceResult(num_bigint::BigInt::zero()))
    }
}

pub async fn eth_get_balance<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<BigInt, ServerError> {
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
) -> Result<LotusJson<EthSyncingResult>, ServerError> {
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
            _ => Err(ServerError::internal_error(
                "missing syncing information, try again",
                None,
            )),
        },
        None => Err(ServerError::internal_error("sync state not found", None)),
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

async fn execute_tipset<DB: Blockstore + Send + Sync + 'static>(
    data: Ctx<DB>,
    tipset: &Arc<Tipset>,
) -> Result<(Cid, Vec<ChainMessage>, Vec<ApiReceipt>)> {
    let msgs = data.chain_store.messages_for_tipset(tipset)?;

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

fn is_eth_address(addr: &VmAddress) -> bool {
    if addr.protocol() != Protocol::Delegated {
        return false;
    }
    let f4_addr: Result<DelegatedAddress, _> = addr.payload().try_into();

    f4_addr.is_ok()
}

fn eth_tx_args_from_unsigned_eth_message(msg: &Message) -> Result<TxArgs> {
    let mut to = Address::default();
    let mut params = vec![];

    if msg.version != 0 {
        bail!("unsupported msg version: {}", msg.version);
    }

    if !msg.params().bytes().is_empty() {
        // TODO: could we do better?
        let mut reader = SliceReader::new(msg.params().bytes());
        match Value::decode(&mut reader) {
            Ok(Value::Bytes(bytes)) => params = bytes,
            _ => bail!("failed to read params byte array"),
        }
    }

    if msg.to == FilecoinAddress::ETHEREUM_ACCOUNT_MANAGER_ACTOR {
        if msg.method_num() != EAMMethod::CreateExternal as u64 {
            bail!("unsupported EAM method");
        }
    } else if msg.method_num() == EVMMethod::InvokeContract as u64 {
        let addr = Address::from_filecoin_address(&msg.to)?;
        to = addr;
    } else {
        bail!(
            "invalid methodnum {}: only allowed method is InvokeContract({})",
            msg.method_num(),
            EVMMethod::InvokeContract as u64
        );
    }

    Ok(TxArgs {
        nonce: msg.sequence,
        to,
        value: msg.value.clone().into(),
        max_fee_per_gas: msg.gas_fee_cap.clone().into(),
        max_priority_fee_per_gas: msg.gas_premium.clone().into(),
        gas_limit: msg.gas_limit,
        input: params,
        ..TxArgs::default()
    })
}

fn recover_sig(sig: &Signature) -> Result<(BigInt, BigInt, BigInt)> {
    if sig.signature_type() != SignatureType::Delegated {
        bail!("recover_sig only supports Delegated signature");
    }

    let len = sig.bytes().len();
    if len != 65 {
        bail!("signature should be 65 bytes long, but got {len} bytes");
    }

    let bytes = sig.bytes();

    #[allow(clippy::indexing_slicing)]
    {
        let r = num_bigint::BigInt::from_bytes_be(Sign::Plus, &bytes[0..32]);

        let s = num_bigint::BigInt::from_bytes_be(Sign::Plus, &bytes[32..64]);

        let v = num_bigint::BigInt::from_bytes_be(Sign::Plus, &bytes[64..65]);

        Ok((BigInt(r), BigInt(s), BigInt(v)))
    }
}

/// `eth_tx_from_signed_eth_message` does NOT populate:
/// - `hash`
/// - `block_hash`
/// - `block_number`
/// - `transaction_index`
fn eth_tx_from_signed_eth_message(smsg: &SignedMessage, chain_id: u32) -> Result<Tx> {
    // The from address is always an f410f address, never an ID or other address.
    let from = smsg.message().from;
    if !is_eth_address(&from) {
        bail!("sender must be an eth account, was {from}");
    }

    // Probably redundant, but we might as well check.
    let sig_type = smsg.signature().signature_type();
    if sig_type != SignatureType::Delegated {
        bail!("signature is not delegated type, is type: {sig_type}");
    }

    let tx_args = eth_tx_args_from_unsigned_eth_message(smsg.message())?;

    let (r, s, v) = recover_sig(smsg.signature())?;

    // This should be impossible to fail as we've already asserted that we have an
    // Ethereum Address sender...
    let from = Address::from_filecoin_address(&from)?;

    Ok(Tx {
        nonce: Uint64(tx_args.nonce),
        chain_id: Uint64(chain_id as u64),
        to: tx_args.to,
        from,
        value: tx_args.value,
        r#type: Uint64(EIP_1559_TX_TYPE),
        gas: Uint64(tx_args.gas_limit),
        max_fee_per_gas: tx_args.max_fee_per_gas,
        max_priority_fee_per_gas: tx_args.max_priority_fee_per_gas,
        access_list: vec![],
        v,
        r,
        s,
        input: Bytes(tx_args.input),
        ..Tx::default()
    })
}

fn lookup_eth_address<DB: Blockstore>(
    addr: &FilecoinAddress,
    state: &StateTree<DB>,
) -> Result<Address> {
    // Attempt to convert directly, if it's an f4 address.
    if let Ok(eth_addr) = Address::from_filecoin_address(addr) {
        if !eth_addr.is_masked_id() {
            return Ok(eth_addr);
        }
    }

    // Otherwise, resolve the ID addr.
    let id_addr = state.lookup_id(addr)?;

    // Lookup on the target actor and try to get an f410 address.
    if let Some(actor_state) = state.get_actor(addr)? {
        if let Some(addr) = actor_state.delegated_address {
            if let Ok(eth_addr) = Address::from_filecoin_address(&addr.into()) {
                if !eth_addr.is_masked_id() {
                    // Conversable into an eth address, use it.
                    return Ok(eth_addr);
                }
            }
        } else {
            // No delegated address -> use a masked ID address
        }
    } else {
        // Not found -> use a masked ID address
    }

    // Otherwise, use the masked address.
    Ok(Address::from_actor_id(id_addr.unwrap()))
}

fn encode_filecoin_params_as_abi(
    method: MethodNum,
    codec: u64,
    params: &fvm_ipld_encoding::RawBytes,
) -> Result<Bytes> {
    let mut buffer: Vec<u8> = vec![0x86, 0x8e, 0x10, 0xc4];
    buffer.append(&mut encode_filecoin_returns_as_abi(method, codec, params));
    Ok(Bytes(buffer))
}

fn encode_filecoin_returns_as_abi(
    exit_code: u64,
    codec: u64,
    data: &fvm_ipld_encoding::RawBytes,
) -> Vec<u8> {
    encode_as_abi_helper(exit_code, codec, data)
}

/// Format 2 numbers followed by an arbitrary byte array as solidity ABI. Both our native
/// inputs/outputs follow the same pattern, so we can reuse this code.
fn encode_as_abi_helper(param1: u64, param2: u64, data: &[u8]) -> Vec<u8> {
    const EVM_WORD_SIZE: usize = 32;

    // The first two params are "static" numbers. Then, we record the offset of the "data" arg,
    // then, at that offset, we record the length of the data.
    //
    // In practice, this means we have 4 256-bit words back to back where the third arg (the
    // offset) is _always_ '32*3'.
    let static_args = [
        param1,
        param2,
        (EVM_WORD_SIZE * 3) as u64,
        data.len() as u64,
    ];
    // We always pad out to the next EVM "word" (32 bytes).
    let total_words = static_args.len()
        + (data.len() / EVM_WORD_SIZE)
        + if (data.len() % EVM_WORD_SIZE) != 0 {
            1
        } else {
            0
        };
    let len = total_words * EVM_WORD_SIZE;
    let mut buf = vec![0u8; len];
    let mut offset = 0;
    // Below, we use copy instead of "appending" to preserve all the zero padding.
    for arg in static_args.iter() {
        // Write each "arg" into the last 8 bytes of each 32 byte word.
        offset += EVM_WORD_SIZE;
        let start = offset - 8;
        buf[start..offset].copy_from_slice(&arg.to_be_bytes());
    }

    // Finally, we copy in the data.
    let data_len = data.len();
    buf[offset..offset + data_len].copy_from_slice(data);

    buf
}

/// Decodes the payload using the given codec.
fn decode_payload(payload: &fvm_ipld_encoding::RawBytes, codec: u64) -> Result<Bytes> {
    match codec {
        // TODO: handle IDENTITY?
        DAG_CBOR | CBOR => {
            let result: Result<Vec<u8>, _> = serde_ipld_dagcbor::de::from_reader(payload.reader());
            match result {
                Ok(buffer) => Ok(Bytes(buffer)),
                Err(err) => bail!("decode_payload: failed to decode cbor payload: {err}"),
            }
        }
        IPLD_RAW => Ok(Bytes(payload.to_vec())),
        _ => bail!("decode_payload: unsupported codec {codec}"),
    }
}

/// Convert a native message to an eth transaction.
///
///   - The state-tree must be from after the message was applied (ideally the following tipset).
///   - In some cases, the "to" address may be `0xff0000000000000000000000ffffffffffffffff`. This
///     means that the "to" address has not been assigned in the passed state-tree and can only
///     happen if the transaction reverted.
///
/// `eth_tx_from_native_message` does NOT populate:
/// - `hash`
/// - `block_hash`
/// - `block_number`
/// - `transaction_index`
fn eth_tx_from_native_message<DB: Blockstore>(
    msg: &Message,
    state: &StateTree<DB>,
    chain_id: u32,
) -> Result<Tx> {
    // Lookup the from address. This must succeed.
    let from = lookup_eth_address(&msg.from(), state).with_context(|| {
        format!(
            "failed to lookup sender address {} when converting a native message to an eth txn",
            msg.from()
        )
    })?;
    // Lookup the to address. If the recipient doesn't exist, we replace the address with a
    // known sentinel address.
    let mut to = match lookup_eth_address(&msg.to(), state) {
        Ok(addr) => addr,
        Err(_err) => {
            // TODO: bail in case of not "actor not found" errors
            Address(ethereum_types::H160::from_str(REVERTED_ETH_ADDRESS)?)

            // bail!(
            //     "failed to lookup receiver address {} when converting a native message to an eth txn",
            //     msg.to()
            // )
        }
    };

    // Finally, convert the input parameters to "solidity ABI".

    // For empty, we use "0" as the codec. Otherwise, we use CBOR for message
    // parameters.
    let codec = if !msg.params().is_empty() { CBOR } else { 0 };

    // We try to decode the input as an EVM method invocation and/or a contract creation. If
    // that fails, we encode the "native" parameters as Solidity ABI.
    let input = 'decode: {
        if msg.method_num() == EVMMethod::InvokeContract as MethodNum
            || msg.method_num() == EAMMethod::CreateExternal as MethodNum
        {
            if let Ok(buffer) = decode_payload(msg.params(), codec) {
                // If this is a valid "create external", unset the "to" address.
                if msg.method_num() == EAMMethod::CreateExternal as MethodNum {
                    // to = None;
                }
                break 'decode buffer;
            }
            // Yeah, we're going to ignore errors here because the user can send whatever they
            // want and may send garbage.
        }
        encode_filecoin_params_as_abi(msg.method_num(), codec, msg.params())?
    };

    let mut tx = Tx::default();
    tx.to = to;
    tx.from = from;
    tx.input = input;
    tx.nonce = Uint64(msg.sequence);
    tx.chain_id = Uint64(chain_id as u64);
    tx.value = msg.value.clone().into();
    tx.r#type = Uint64(EIP_1559_TX_TYPE);
    tx.gas = Uint64(msg.gas_limit);
    tx.max_fee_per_gas = msg.gas_fee_cap.clone().into();
    tx.max_priority_fee_per_gas = msg.gas_premium.clone().into();
    tx.access_list = vec![];

    Ok(tx)
}

pub fn new_eth_tx_from_signed_message<DB: Blockstore>(
    smsg: &SignedMessage,
    state: &StateTree<DB>,
    chain_id: u32,
) -> Result<Tx> {
    let mut tx: Tx = Tx::default();
    tx.chain_id = Uint64(1);

    if smsg.is_delegated() {
        // This is an eth tx
        tx = eth_tx_from_signed_eth_message(smsg, chain_id)?;
        tx.hash = tx.eth_hash();
    } else if smsg.is_secp256k1() {
        // Secp Filecoin Message
        tx = eth_tx_from_native_message(smsg.message(), state, chain_id)?;
        tx.hash = smsg.cid()?.into();
    } else {
        // BLS Filecoin message
        tx = eth_tx_from_native_message(smsg.message(), state, chain_id)?;
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
    let mut transaction_hashes = vec![];
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

        let mut tx = new_eth_tx_from_signed_message(
            &smsg,
            &state_tree,
            data.state_manager.chain_config().eth_chain_id,
        )?;
        tx.block_hash = block_hash.clone();
        tx.block_number = block_number.clone();
        tx.transaction_index = ti;

        if full_tx_info {
            transactions.push(tx);
        } else {
            transaction_hashes.push(tx.hash.to_string());
        }
    }

    let mut block = Block::new();
    block.hash = block_hash;
    block.number = block_number;
    block.parent_hash = parent_cid.into();
    block.timestamp = Uint64(tipset.block_headers().first().timestamp);
    block.base_fee_per_gas = tipset
        .block_headers()
        .first()
        .parent_base_fee
        .clone()
        .into();
    block.gas_used = Uint64(gas_used);
    block.transactions = if full_tx_info {
        Transactions::Full(transactions)
    } else {
        Transactions::Hash(transaction_hashes)
    };

    Ok(block)
}

pub async fn eth_get_block_by_number<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<Block, ServerError> {
    let LotusJson((block_param, full_tx_info)): LotusJson<(BlockNumberOrHash, bool)> =
        params.parse()?;

    let ts = tipset_by_block_number_or_hash(&data.chain_store, block_param)?;

    let block = block_from_filecoin_tipset(data, ts, full_tx_info).await?;

    Ok(block)
}

#[cfg(test)]
mod test {
    use super::*;
    use quickcheck_macros::quickcheck;
    use std::num::ParseIntError;

    #[quickcheck]
    fn gas_price_result_serde_roundtrip(i: u128) {
        let r = GasPriceResult(i.into());
        let encoded = serde_json::to_string(&r).unwrap();
        assert_eq!(encoded, format!("\"{i:#x}\""));
        let decoded: GasPriceResult = serde_json::from_str(&encoded).unwrap();
        assert_eq!(r.0, decoded.0);
    }

    pub fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
            .collect()
    }

    #[test]
    fn test_abi_encoding() {
        const EXPECTED: &str = "000000000000000000000000000000000000000000000000000000000000001600000000000000000000000000000000000000000000000000000000000000510000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000001b1111111111111111111020200301000000044444444444444444010000000000";
        const DATA: &str = "111111111111111111102020030100000004444444444444444401";
        let expected_bytes = decode_hex(EXPECTED).unwrap();
        let data_bytes = decode_hex(DATA).unwrap();

        assert_eq!(expected_bytes, encode_as_abi_helper(22, 81, &data_bytes));
    }

    #[test]
    fn test_rlp_encoding() {
        let eth_tx_args = TxArgs {
            chain_id: 314159,
            nonce: 486,
            to: Address(
                ethereum_types::H160::from_str("0xeb4a9cdb9f42d3a503d580a39b6e3736eb21fffd")
                    .unwrap(),
            ),
            value: BigInt(num_bigint::BigInt::from(0)),
            max_fee_per_gas: BigInt(num_bigint::BigInt::from(1500000120)),
            max_priority_fee_per_gas: BigInt(num_bigint::BigInt::from(1500000000)),
            gas_limit: 37442471,
            input: decode_hex("383487be000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000660d4d120000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000003b6261666b726569656f6f75326d36356276376561786e7767656d7562723675787269696867366474646e6c7a663469616f37686c6e6a6d647372750000000000").unwrap(),
            v: BigInt(num_bigint::BigInt::from_str("1").unwrap()),
            r: BigInt(
                num_bigint::BigInt::from_str(
                    "84103132941276310528712440865285269631208564772362393569572880532520338257200",
                )
                .unwrap(),
            ),
            s: BigInt(
                num_bigint::BigInt::from_str(
                    "7820796778417228639067439047870612492553874254089570360061550763595363987236",
                )
                .unwrap(),
            ),
        };

        let expected_hash = Hash(
            ethereum_types::H256::from_str(
                "0x9f2e70d5737c6b798eccea14895893fb48091ab3c59d0fe95508dc7efdae2e5f",
            )
            .unwrap(),
        );
        assert_eq!(expected_hash, eth_tx_args.hash());
    }
}

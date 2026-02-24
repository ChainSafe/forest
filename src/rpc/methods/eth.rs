// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub(crate) mod errors;
mod eth_tx;
pub mod filter;
pub mod pubsub;
pub(crate) mod pubsub_trait;
mod trace;
pub mod types;
mod utils;

use self::eth_tx::*;
use self::filter::hex_str_to_epoch;
use self::types::*;
use super::gas;
use crate::blocks::{Tipset, TipsetKey};
use crate::chain::{ChainStore, index::ResolveNullTipset};
use crate::chain_sync::NodeSyncStatus;
use crate::cid_collections::CidHashSet;
use crate::eth::{
    EAMMethod, EVMMethod, EthChainId as EthChainIdType, EthEip1559TxArgs, EthLegacyEip155TxArgs,
    EthLegacyHomesteadTxArgs,
};
use crate::eth::{SAFE_EPOCH_DELAY, parse_eth_transaction};
use crate::interpreter::VMTrace;
use crate::lotus_json::{HasLotusJson, lotus_json_with_self};
use crate::message::{ChainMessage, Message as _, SignedMessage};
use crate::rpc::error::ServerError;
use crate::rpc::eth::errors::EthErrors;
use crate::rpc::eth::filter::{
    SkipEvent, event::EventFilter, mempool::MempoolFilter, tipset::TipSetFilter,
};
use crate::rpc::eth::types::{EthBlockTrace, EthTrace};
use crate::rpc::eth::utils::decode_revert_reason;
use crate::rpc::methods::chain::ChainGetTipSetV2;
use crate::rpc::state::ApiInvocResult;
use crate::rpc::types::{ApiTipsetKey, EventEntry, MessageLookup};
use crate::rpc::{ApiPaths, Ctx, Permission, RpcMethod};
use crate::rpc::{EthEventHandler, LOOKBACK_NO_LIMIT};
use crate::shim::actors::EVMActorStateLoad as _;
use crate::shim::actors::eam;
use crate::shim::actors::evm;
use crate::shim::actors::is_evm_actor;
use crate::shim::actors::system;
use crate::shim::address::{Address as FilecoinAddress, Protocol};
use crate::shim::crypto::Signature;
use crate::shim::econ::{BLOCK_GAS_LIMIT, TokenAmount};
use crate::shim::error::ExitCode;
use crate::shim::executor::Receipt;
use crate::shim::fvm_shared_latest::MethodNum;
use crate::shim::fvm_shared_latest::address::{Address as VmAddress, DelegatedAddress};
use crate::shim::gas::GasOutputs;
use crate::shim::message::Message;
use crate::shim::trace::{CallReturn, ExecutionEvent};
use crate::shim::{clock::ChainEpoch, state_tree::StateTree};
use crate::state_manager::{StateLookupPolicy, VMFlush};
use crate::utils::cache::SizeTrackingLruCache;
use crate::utils::db::BlockstoreExt as _;
use crate::utils::encoding::from_slice_with_fallback;
use crate::utils::get_size::{CidWrapper, big_int_heap_size_helper};
use crate::utils::misc::env::env_or_default;
use crate::utils::multihash::prelude::*;
use ahash::HashSet;
use anyhow::{Context, Error, Result, anyhow, bail, ensure};
use cid::Cid;
use enumflags2::{BitFlags, make_bitflags};
use filter::{ParsedFilter, ParsedFilterTipsets};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{CBOR, DAG_CBOR, IPLD_RAW, RawBytes};
use get_size2::GetSize;
use ipld_core::ipld::Ipld;
use itertools::Itertools;
use nonzero_ext::nonzero;
use num::{BigInt, Zero as _};
use nunny::Vec as NonEmpty;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::ops::RangeInclusive;
use std::str::FromStr;
use std::sync::{Arc, LazyLock};
use utils::{decode_payload, lookup_eth_address};

static FOREST_TRACE_FILTER_MAX_RESULT: LazyLock<u64> =
    LazyLock::new(|| env_or_default("FOREST_TRACE_FILTER_MAX_RESULT", 500));

const MASKED_ID_PREFIX: [u8; 12] = [0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// Ethereum Bloom filter size in bits.
/// Bloom filter is used in Ethereum to minimize the number of block queries.
const BLOOM_SIZE: usize = 2048;

/// Ethereum Bloom filter size in bytes.
const BLOOM_SIZE_IN_BYTES: usize = BLOOM_SIZE / 8;

/// Ethereum Bloom filter with all bits set to 1.
const FULL_BLOOM: [u8; BLOOM_SIZE_IN_BYTES] = [0xff; BLOOM_SIZE_IN_BYTES];

/// Ethereum Bloom filter with all bits set to 0.
const EMPTY_BLOOM: [u8; BLOOM_SIZE_IN_BYTES] = [0x0; BLOOM_SIZE_IN_BYTES];

/// Ethereum address size in bytes.
const ADDRESS_LENGTH: usize = 20;

/// Ethereum Virtual Machine word size in bytes.
const EVM_WORD_LENGTH: usize = 32;

/// Keccak-256 of an RLP of an empty array.
/// In Filecoin, we don't have the concept of uncle blocks but rather use tipsets to reward miners
/// who craft blocks.
const EMPTY_UNCLES: &str = "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347";

/// Keccak-256 of the RLP of null.
const EMPTY_ROOT: &str = "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421";

/// The address used in messages to actors that have since been deleted.
const REVERTED_ETH_ADDRESS: &str = "0xff0000000000000000000000ffffffffffffffff";

// TODO(forest): https://github.com/ChainSafe/forest/issues/4436
//               use ethereum_types::U256 or use lotus_json::big_int
#[derive(
    Eq,
    Hash,
    PartialEq,
    Debug,
    Deserialize,
    Serialize,
    Default,
    Clone,
    JsonSchema,
    derive_more::From,
    derive_more::Into,
)]
pub struct EthBigInt(
    #[serde(with = "crate::lotus_json::hexify")]
    #[schemars(with = "String")]
    pub BigInt,
);
lotus_json_with_self!(EthBigInt);

impl GetSize for EthBigInt {
    fn get_heap_size(&self) -> usize {
        big_int_heap_size_helper(&self.0)
    }
}

impl From<TokenAmount> for EthBigInt {
    fn from(amount: TokenAmount) -> Self {
        (&amount).into()
    }
}

impl From<&TokenAmount> for EthBigInt {
    fn from(amount: &TokenAmount) -> Self {
        Self(amount.atto().to_owned())
    }
}

type GasPriceResult = EthBigInt;

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone, JsonSchema)]
pub struct Nonce(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::hexify_bytes")]
    pub ethereum_types::H64,
);
lotus_json_with_self!(Nonce);

impl GetSize for Nonce {
    fn get_heap_size(&self) -> usize {
        0
    }
}

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone, JsonSchema)]
pub struct Bloom(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::hexify_bytes")]
    pub ethereum_types::Bloom,
);
lotus_json_with_self!(Bloom);

impl GetSize for Bloom {
    fn get_heap_size(&self) -> usize {
        0
    }
}

impl Bloom {
    pub fn accrue(&mut self, input: &[u8]) {
        self.0.accrue(ethereum_types::BloomInput::Raw(input));
    }
}

#[derive(
    Eq,
    Hash,
    PartialEq,
    Debug,
    Deserialize,
    Serialize,
    Default,
    Clone,
    Copy,
    JsonSchema,
    derive_more::From,
    derive_more::Into,
    derive_more::Deref,
    GetSize,
)]
pub struct EthUint64(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::hexify")]
    pub u64,
);

lotus_json_with_self!(EthUint64);

impl EthUint64 {
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != EVM_WORD_LENGTH {
            bail!("eth int must be {EVM_WORD_LENGTH} bytes");
        }

        // big endian format stores u64 in the last 8 bytes,
        // since ethereum words are 32 bytes, the first 24 bytes must be 0
        if data
            .get(..24)
            .is_none_or(|slice| slice.iter().any(|&byte| byte != 0))
        {
            bail!("eth int overflows 64 bits");
        }

        // Extract the uint64 from the last 8 bytes
        Ok(Self(u64::from_be_bytes(
            data.get(24..EVM_WORD_LENGTH)
                .ok_or_else(|| anyhow::anyhow!("data too short"))?
                .try_into()?,
        )))
    }

    pub fn to_hex_string(self) -> String {
        format!("0x{}", hex::encode(self.0.to_be_bytes()))
    }
}

#[derive(
    PartialEq,
    Debug,
    Deserialize,
    Serialize,
    Default,
    Clone,
    Copy,
    JsonSchema,
    derive_more::From,
    derive_more::Into,
    derive_more::Deref,
)]
pub struct EthInt64(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::hexify")]
    pub i64,
);

lotus_json_with_self!(EthInt64);

impl EthHash {
    // Should ONLY be used for blocks and Filecoin messages. Eth transactions expect a different hashing scheme.
    pub fn to_cid(self) -> cid::Cid {
        let mh = MultihashCode::Blake2b256
            .wrap(self.0.as_bytes())
            .expect("should not fail");
        Cid::new_v1(DAG_CBOR, mh)
    }

    pub fn empty_uncles() -> Self {
        Self(ethereum_types::H256::from_str(EMPTY_UNCLES).unwrap())
    }

    pub fn empty_root() -> Self {
        Self(ethereum_types::H256::from_str(EMPTY_ROOT).unwrap())
    }
}

impl From<Cid> for EthHash {
    fn from(cid: Cid) -> Self {
        let (_, digest, _) = cid.hash().into_inner();
        EthHash(ethereum_types::H256::from_slice(&digest[0..32]))
    }
}

impl From<[u8; EVM_WORD_LENGTH]> for EthHash {
    fn from(value: [u8; EVM_WORD_LENGTH]) -> Self {
        Self(ethereum_types::H256(value))
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Predefined {
    Earliest,
    Pending,
    #[default]
    Latest,
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ExtPredefined {
    Earliest,
    Pending,
    #[default]
    Latest,
    Safe,
    Finalized,
}

impl TryFrom<&ExtPredefined> for Predefined {
    type Error = ();
    fn try_from(ext: &ExtPredefined) -> Result<Self, Self::Error> {
        match ext {
            ExtPredefined::Earliest => Ok(Predefined::Earliest),
            ExtPredefined::Pending => Ok(Predefined::Pending),
            ExtPredefined::Latest => Ok(Predefined::Latest),
            _ => Err(()),
        }
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlockNumber {
    block_number: EthInt64,
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlockHash {
    block_hash: EthHash,
    #[serde(default)]
    require_canonical: bool,
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BlockNumberOrHash {
    #[schemars(with = "String")]
    PredefinedBlock(Predefined),
    BlockNumber(EthInt64),
    BlockHash(EthHash),
    BlockNumberObject(BlockNumber),
    BlockHashObject(BlockHash),
}

lotus_json_with_self!(BlockNumberOrHash);

impl BlockNumberOrHash {
    pub fn from_predefined(predefined: Predefined) -> Self {
        Self::PredefinedBlock(predefined)
    }

    pub fn from_block_number(number: i64) -> Self {
        Self::BlockNumber(EthInt64(number))
    }

    pub fn from_block_hash(hash: EthHash) -> Self {
        Self::BlockHash(hash)
    }

    /// Construct a block number using EIP-1898 Object scheme.
    ///
    /// For details see <https://eips.ethereum.org/EIPS/eip-1898>
    pub fn from_block_number_object(number: i64) -> Self {
        Self::BlockNumberObject(BlockNumber {
            block_number: EthInt64(number),
        })
    }

    /// Construct a block hash using EIP-1898 Object scheme.
    ///
    /// For details see <https://eips.ethereum.org/EIPS/eip-1898>
    pub fn from_block_hash_object(hash: EthHash, require_canonical: bool) -> Self {
        Self::BlockHashObject(BlockHash {
            block_hash: hash,
            require_canonical,
        })
    }

    pub fn from_str(s: &str) -> Result<Self, Error> {
        match s {
            "earliest" => Ok(BlockNumberOrHash::from_predefined(Predefined::Earliest)),
            "pending" => Ok(BlockNumberOrHash::from_predefined(Predefined::Pending)),
            "latest" | "" => Ok(BlockNumberOrHash::from_predefined(Predefined::Latest)),
            hex if hex.starts_with("0x") => {
                let epoch = hex_str_to_epoch(hex)?;
                Ok(BlockNumberOrHash::from_block_number(epoch))
            }
            _ => Err(anyhow!("Invalid block identifier")),
        }
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ExtBlockNumberOrHash {
    #[schemars(with = "String")]
    PredefinedBlock(ExtPredefined),
    BlockNumber(EthInt64),
    BlockHash(EthHash),
    BlockNumberObject(BlockNumber),
    BlockHashObject(BlockHash),
}

lotus_json_with_self!(ExtBlockNumberOrHash);

#[allow(dead_code)]
impl ExtBlockNumberOrHash {
    pub fn from_predefined(ext_predefined: ExtPredefined) -> Self {
        Self::PredefinedBlock(ext_predefined)
    }

    pub fn from_block_number(number: i64) -> Self {
        Self::BlockNumber(EthInt64(number))
    }

    pub fn from_block_hash(hash: EthHash) -> Self {
        Self::BlockHash(hash)
    }

    /// Construct a block number using EIP-1898 Object scheme.
    ///
    /// For details see <https://eips.ethereum.org/EIPS/eip-1898>
    pub fn from_block_number_object(number: i64) -> Self {
        Self::BlockNumberObject(BlockNumber {
            block_number: EthInt64(number),
        })
    }

    /// Construct a block hash using EIP-1898 Object scheme.
    ///
    /// For details see <https://eips.ethereum.org/EIPS/eip-1898>
    pub fn from_block_hash_object(hash: EthHash, require_canonical: bool) -> Self {
        Self::BlockHashObject(BlockHash {
            block_hash: hash,
            require_canonical,
        })
    }

    pub fn from_str(s: &str) -> Result<Self, Error> {
        match s {
            "earliest" => Ok(ExtBlockNumberOrHash::from_predefined(
                ExtPredefined::Earliest,
            )),
            "pending" => Ok(ExtBlockNumberOrHash::from_predefined(
                ExtPredefined::Pending,
            )),
            "latest" | "" => Ok(ExtBlockNumberOrHash::from_predefined(ExtPredefined::Latest)),
            "safe" => Ok(ExtBlockNumberOrHash::from_predefined(ExtPredefined::Safe)),
            "finalized" => Ok(ExtBlockNumberOrHash::from_predefined(
                ExtPredefined::Finalized,
            )),
            hex if hex.starts_with("0x") => {
                let epoch = hex_str_to_epoch(hex)?;
                Ok(ExtBlockNumberOrHash::from_block_number(epoch))
            }
            _ => Err(anyhow!("Invalid block identifier")),
        }
    }
}

/// Selects which trace outputs to include in the `trace_call` response.
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum EthTraceType {
    /// Requests a structured call graph, showing the hierarchy of calls (e.g., `call`, `create`, `reward`)
    /// with details like `from`, `to`, `gas`, `input`, `output`, and `subtraces`.
    Trace,
    /// Requests a state difference object, detailing changes to account states (e.g., `balance`, `nonce`, `storage`, `code`)
    /// caused by the simulated transaction.
    ///
    /// It shows `"from"` and `"to"` values for modified fields, using `"+"`, `"-"`, or `"="` for code changes.
    StateDiff,
}

lotus_json_with_self!(EthTraceType);

/// Result payload returned by `trace_call`.
#[derive(PartialEq, Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthTraceResults {
    /// Output bytes from the transaction execution.
    pub output: EthBytes,
    /// State diff showing all account changes.
    pub state_diff: Option<StateDiff>,
    /// Call trace hierarchy (empty when not requested).
    #[serde(default)]
    pub trace: Vec<EthTrace>,
}

lotus_json_with_self!(EthTraceResults);

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, GetSize)]
#[serde(untagged)] // try a Vec<String>, then a Vec<Tx>
pub enum Transactions {
    Hash(Vec<String>),
    Full(Vec<ApiEthTx>),
}

impl Transactions {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Hash(v) => v.is_empty(),
            Self::Full(v) => v.is_empty(),
        }
    }
}

impl PartialEq for Transactions {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Hash(a), Self::Hash(b)) => a == b,
            (Self::Full(a), Self::Full(b)) => a == b,
            _ => self.is_empty() && other.is_empty(),
        }
    }
}

impl Default for Transactions {
    fn default() -> Self {
        Self::Hash(vec![])
    }
}

#[derive(PartialEq, Debug, Clone, Default, Serialize, Deserialize, JsonSchema, GetSize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub hash: EthHash,
    pub parent_hash: EthHash,
    pub sha3_uncles: EthHash,
    pub miner: EthAddress,
    pub state_root: EthHash,
    pub transactions_root: EthHash,
    pub receipts_root: EthHash,
    pub logs_bloom: Bloom,
    pub difficulty: EthUint64,
    pub total_difficulty: EthUint64,
    pub number: EthUint64,
    pub gas_limit: EthUint64,
    pub gas_used: EthUint64,
    pub timestamp: EthUint64,
    pub extra_data: EthBytes,
    pub mix_hash: EthHash,
    pub nonce: Nonce,
    pub base_fee_per_gas: EthBigInt,
    pub size: EthUint64,
    // can be Vec<Tx> or Vec<String> depending on query params
    pub transactions: Transactions,
    pub uncles: Vec<EthHash>,
}

/// Specifies the level of detail for transactions in Ethereum blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxInfo {
    /// Return only transaction hashes
    Hash,
    /// Return full transaction objects
    Full,
}

impl From<bool> for TxInfo {
    fn from(full: bool) -> Self {
        if full { TxInfo::Full } else { TxInfo::Hash }
    }
}

impl Block {
    pub fn new(has_transactions: bool, tipset_len: usize) -> Self {
        Self {
            gas_limit: EthUint64(BLOCK_GAS_LIMIT.saturating_mul(tipset_len as _)),
            logs_bloom: Bloom(ethereum_types::Bloom(FULL_BLOOM)),
            sha3_uncles: EthHash::empty_uncles(),
            transactions_root: if has_transactions {
                EthHash::default()
            } else {
                EthHash::empty_root()
            },
            ..Default::default()
        }
    }

    /// Creates a new Ethereum block from a Filecoin tipset, executing transactions if requested.
    ///
    /// Reference: <https://github.com/filecoin-project/lotus/blob/941455f1d23e73b9ee92a1a4ce745d8848969858/node/impl/eth/utils.go#L44>
    pub async fn from_filecoin_tipset<DB: Blockstore + Send + Sync + 'static>(
        ctx: Ctx<DB>,
        tipset: crate::blocks::Tipset,
        tx_info: TxInfo,
    ) -> Result<Self> {
        static ETH_BLOCK_CACHE: LazyLock<SizeTrackingLruCache<CidWrapper, Block>> =
            LazyLock::new(|| {
                const DEFAULT_CACHE_SIZE: NonZeroUsize = nonzero!(500usize);
                let cache_size = std::env::var("FOREST_ETH_BLOCK_CACHE_SIZE")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(DEFAULT_CACHE_SIZE);
                SizeTrackingLruCache::new_with_metrics("eth_block".into(), cache_size)
            });

        let block_cid = tipset.key().cid()?;
        let mut block = if let Some(b) = ETH_BLOCK_CACHE.get_cloned(&block_cid.into()) {
            b
        } else {
            let parent_cid = tipset.parents().cid()?;
            let block_number = EthUint64(tipset.epoch() as u64);
            let block_hash: EthHash = block_cid.into();

            let (state_root, msgs_and_receipts) = execute_tipset(&ctx, &tipset).await?;

            let state_tree = ctx.state_manager.get_state_tree(&state_root)?;

            let mut full_transactions = vec![];
            let mut gas_used = 0;
            for (i, (msg, receipt)) in msgs_and_receipts.iter().enumerate() {
                let ti = EthUint64(i as u64);
                gas_used += receipt.gas_used();
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
                    ctx.chain_config().eth_chain_id,
                )?;
                tx.block_hash = block_hash;
                tx.block_number = block_number;
                tx.transaction_index = ti;
                full_transactions.push(tx);
            }

            let b = Block {
                hash: block_hash,
                number: block_number,
                parent_hash: parent_cid.into(),
                timestamp: EthUint64(tipset.block_headers().first().timestamp),
                base_fee_per_gas: tipset
                    .block_headers()
                    .first()
                    .parent_base_fee
                    .clone()
                    .into(),
                gas_used: EthUint64(gas_used),
                transactions: Transactions::Full(full_transactions),
                ..Block::new(!msgs_and_receipts.is_empty(), tipset.len())
            };
            ETH_BLOCK_CACHE.push(block_cid.into(), b.clone());
            b
        };

        if tx_info == TxInfo::Hash
            && let Transactions::Full(transactions) = &block.transactions
        {
            block.transactions =
                Transactions::Hash(transactions.iter().map(|tx| tx.hash.to_string()).collect())
        }

        Ok(block)
    }
}

lotus_json_with_self!(Block);

#[derive(PartialEq, Debug, Clone, Default, Serialize, Deserialize, JsonSchema, GetSize)]
#[serde(rename_all = "camelCase")]
pub struct ApiEthTx {
    pub chain_id: EthUint64,
    pub nonce: EthUint64,
    pub hash: EthHash,
    pub block_hash: EthHash,
    pub block_number: EthUint64,
    pub transaction_index: EthUint64,
    pub from: EthAddress,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub to: Option<EthAddress>,
    pub value: EthBigInt,
    pub r#type: EthUint64,
    pub input: EthBytes,
    pub gas: EthUint64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_fee_per_gas: Option<EthBigInt>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_priority_fee_per_gas: Option<EthBigInt>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gas_price: Option<EthBigInt>,
    #[schemars(with = "Option<Vec<EthHash>>")]
    #[serde(with = "crate::lotus_json")]
    pub access_list: Vec<EthHash>,
    pub v: EthBigInt,
    pub r: EthBigInt,
    pub s: EthBigInt,
}
lotus_json_with_self!(ApiEthTx);

impl ApiEthTx {
    fn gas_fee_cap(&self) -> anyhow::Result<EthBigInt> {
        self.max_fee_per_gas
            .as_ref()
            .or(self.gas_price.as_ref())
            .cloned()
            .context("gas fee cap is not set")
    }

    fn gas_premium(&self) -> anyhow::Result<EthBigInt> {
        self.max_priority_fee_per_gas
            .as_ref()
            .or(self.gas_price.as_ref())
            .cloned()
            .context("gas premium is not set")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EthSyncingResult {
    pub done_sync: bool,
    pub starting_block: i64,
    pub current_block: i64,
    pub highest_block: i64,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum EthSyncingResultLotusJson {
    DoneSync(bool),
    Syncing {
        #[schemars(with = "i64")]
        #[serde(rename = "startingblock", with = "crate::lotus_json::hexify")]
        starting_block: i64,
        #[schemars(with = "i64")]
        #[serde(rename = "currentblock", with = "crate::lotus_json::hexify")]
        current_block: i64,
        #[schemars(with = "i64")]
        #[serde(rename = "highestblock", with = "crate::lotus_json::hexify")]
        highest_block: i64,
    },
}

// TODO(forest): https://github.com/ChainSafe/forest/issues/4032
//               this shouldn't exist
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

#[derive(PartialEq, Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthTxReceipt {
    transaction_hash: EthHash,
    transaction_index: EthUint64,
    block_hash: EthHash,
    block_number: EthUint64,
    from: EthAddress,
    to: Option<EthAddress>,
    root: EthHash,
    status: EthUint64,
    contract_address: Option<EthAddress>,
    cumulative_gas_used: EthUint64,
    gas_used: EthUint64,
    effective_gas_price: EthBigInt,
    logs_bloom: EthBytes,
    logs: Vec<EthLog>,
    r#type: EthUint64,
}
lotus_json_with_self!(EthTxReceipt);

impl EthTxReceipt {
    fn new() -> Self {
        Self {
            logs_bloom: EthBytes(EMPTY_BLOOM.to_vec()),
            ..Self::default()
        }
    }
}

/// Represents the results of an event filter execution.
#[derive(PartialEq, Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthLog {
    /// The address of the actor that produced the event log.
    address: EthAddress,
    /// The value of the event log, excluding topics.
    data: EthBytes,
    /// List of topics associated with the event log.
    topics: Vec<EthHash>,
    /// Indicates whether the log was removed due to a chain reorganization.
    removed: bool,
    /// The index of the event log in the sequence of events produced by the message execution.
    /// (this is the index in the events AMT on the message receipt)
    log_index: EthUint64,
    /// The index in the tipset of the transaction that produced the event log.
    /// The index corresponds to the sequence of messages produced by `ChainGetParentMessages`
    transaction_index: EthUint64,
    /// The hash of the RLP message that produced the event log.
    transaction_hash: EthHash,
    /// The hash of the tipset containing the message that produced the log.
    block_hash: EthHash,
    /// The epoch of the tipset containing the message.
    block_number: EthUint64,
}
lotus_json_with_self!(EthLog);

pub enum Web3ClientVersion {}
impl RpcMethod<0> for Web3ClientVersion {
    const NAME: &'static str = "Filecoin.Web3ClientVersion";
    const NAME_ALIAS: Option<&'static str> = Some("web3_clientVersion");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = String;

    async fn handle(
        _: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(format!(
            "forest/{}",
            *crate::utils::version::FOREST_VERSION_STRING
        ))
    }
}

pub enum EthAccounts {}
impl RpcMethod<0> for EthAccounts {
    const NAME: &'static str = "Filecoin.EthAccounts";
    const NAME_ALIAS: Option<&'static str> = Some("eth_accounts");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = Vec<String>;

    async fn handle(
        _: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        // EthAccounts will always return [] since we don't expect Forest to manage private keys
        Ok(vec![])
    }
}

pub enum EthBlockNumber {}
impl RpcMethod<0> for EthBlockNumber {
    const NAME: &'static str = "Filecoin.EthBlockNumber";
    const NAME_ALIAS: Option<&'static str> = Some("eth_blockNumber");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = EthUint64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        // `eth_block_number` needs to return the height of the latest committed tipset.
        // Ethereum clients expect all transactions included in this block to have execution outputs.
        // This is the parent of the head tipset. The head tipset is speculative, has not been
        // recognized by the network, and its messages are only included, not executed.
        // See https://github.com/filecoin-project/ref-fvm/issues/1135.
        let heaviest = ctx.chain_store().heaviest_tipset();
        if heaviest.epoch() == 0 {
            // We're at genesis.
            return Ok(EthUint64::default());
        }
        // First non-null parent.
        let effective_parent = heaviest.parents();
        if let Ok(Some(parent)) = ctx.chain_index().load_tipset(effective_parent) {
            Ok((parent.epoch() as u64).into())
        } else {
            Ok(EthUint64::default())
        }
    }
}

pub enum EthChainId {}
impl RpcMethod<0> for EthChainId {
    const NAME: &'static str = "Filecoin.EthChainId";
    const NAME_ALIAS: Option<&'static str> = Some("eth_chainId");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = String;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(format!("{:#x}", ctx.chain_config().eth_chain_id))
    }
}

pub enum EthGasPrice {}
impl RpcMethod<0> for EthGasPrice {
    const NAME: &'static str = "Filecoin.EthGasPrice";
    const NAME_ALIAS: Option<&'static str> = Some("eth_gasPrice");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns the current gas price in attoFIL");

    type Params = ();
    type Ok = GasPriceResult;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        // According to Geth's implementation, eth_gasPrice should return base + tip
        // Ref: https://github.com/ethereum/pm/issues/328#issuecomment-853234014
        let ts = ctx.chain_store().heaviest_tipset();
        let block0 = ts.block_headers().first();
        let base_fee = block0.parent_base_fee.atto();
        let tip = crate::rpc::gas::estimate_gas_premium(&ctx, 0, &ApiTipsetKey(None))
            .await
            .map(|gas_premium| gas_premium.atto().to_owned())
            .unwrap_or_default();
        Ok(EthBigInt(base_fee + tip))
    }
}

pub enum EthGetBalance {}
impl RpcMethod<2> for EthGetBalance {
    const NAME: &'static str = "Filecoin.EthGetBalance";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getBalance");
    const PARAM_NAMES: [&'static str; 2] = ["address", "blockParam"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the balance of an Ethereum address at the specified block state");

    type Params = (EthAddress, BlockNumberOrHash);
    type Ok = EthBigInt;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, block_param): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash(
            ctx.chain_store(),
            block_param,
            ResolveNullTipset::TakeOlder,
        )?;
        let balance = eth_get_balance(&ctx, &address, &ts).await?;
        Ok(balance)
    }
}

pub enum EthGetBalanceV2 {}
impl RpcMethod<2> for EthGetBalanceV2 {
    const NAME: &'static str = "Filecoin.EthGetBalance";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getBalance");
    const PARAM_NAMES: [&'static str; 2] = ["address", "blockParam"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V2);
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the balance of an Ethereum address at the specified block state");

    type Params = (EthAddress, ExtBlockNumberOrHash);
    type Ok = EthBigInt;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, block_param): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash_v2(&ctx, block_param, ResolveNullTipset::TakeOlder)
            .await?;
        let balance = eth_get_balance(&ctx, &address, &ts).await?;
        Ok(balance)
    }
}

async fn eth_get_balance<DB: Blockstore + Send + Sync + 'static>(
    ctx: &Ctx<DB>,
    address: &EthAddress,
    ts: &Tipset,
) -> Result<EthBigInt> {
    let fil_addr = address.to_filecoin_address()?;
    let (state_cid, _) = ctx
        .state_manager
        .tipset_state(ts, StateLookupPolicy::Enabled)
        .await?;
    let state_tree = ctx.state_manager.get_state_tree(&state_cid)?;
    match state_tree.get_actor(&fil_addr)? {
        Some(actor) => Ok(EthBigInt(actor.balance.atto().clone())),
        None => Ok(EthBigInt::default()), // Balance is 0 if the actor doesn't exist
    }
}

fn get_tipset_from_hash<DB: Blockstore>(
    chain_store: &ChainStore<DB>,
    block_hash: &EthHash,
) -> anyhow::Result<Tipset> {
    let tsk = chain_store.get_required_tipset_key(block_hash)?;
    Tipset::load_required(chain_store.blockstore(), &tsk)
}

fn resolve_predefined_tipset<DB: Blockstore>(
    chain: &ChainStore<DB>,
    head: Tipset,
    predefined: Predefined,
) -> anyhow::Result<Tipset> {
    match predefined {
        Predefined::Earliest => bail!("block param \"earliest\" is not supported"),
        Predefined::Pending => Ok(head),
        Predefined::Latest => Ok(chain.chain_index().load_required_tipset(head.parents())?),
    }
}

async fn resolve_predefined_tipset_v2<DB: Blockstore + Send + Sync + 'static>(
    ctx: &Ctx<DB>,
    head: Tipset,
    tag: ExtPredefined,
) -> anyhow::Result<Tipset> {
    if let Ok(common) = Predefined::try_from(&tag) {
        resolve_predefined_tipset(ctx.chain_store(), head, common)
    } else {
        match tag {
            ExtPredefined::Safe => Ok(ChainGetTipSetV2::get_latest_safe_tipset(ctx).await?),
            ExtPredefined::Finalized => Ok(ChainGetTipSetV2::get_latest_finalized_tipset(ctx)
                .await?
                .unwrap_or(ctx.chain_index().tipset_by_height(
                    0,
                    head,
                    ResolveNullTipset::TakeOlder,
                )?)),
            _ => bail!("unknown block tag: {:?}", tag),
        }
    }
}

fn resolve_ext_predefined_tipset<DB: Blockstore>(
    chain: &ChainStore<DB>,
    head: Tipset,
    ext_predefined: ExtPredefined,
    resolve: ResolveNullTipset,
) -> anyhow::Result<Tipset> {
    if let Ok(common) = Predefined::try_from(&ext_predefined) {
        resolve_predefined_tipset(chain, head, common)
    } else {
        let latest_height = head.epoch() - 1;
        // Matches all `ExtPredefined` variants outside `Predefined`.
        match ext_predefined {
            ExtPredefined::Safe => {
                let safe_height = latest_height - SAFE_EPOCH_DELAY;
                Ok(chain
                    .chain_index()
                    .tipset_by_height(safe_height, head, resolve)?)
            }
            ExtPredefined::Finalized => {
                let finality_height = latest_height - chain.chain_config().policy.chain_finality;
                Ok(chain
                    .chain_index()
                    .tipset_by_height(finality_height, head, resolve)?)
            }
            _ => bail!("Unhandled ExtPredefined variant: {:?}", ext_predefined),
        }
    }
}

fn resolve_block_number_tipset<DB: Blockstore>(
    chain: &ChainStore<DB>,
    head: Tipset,
    block_number: EthInt64,
    resolve: ResolveNullTipset,
) -> anyhow::Result<Tipset> {
    let height = ChainEpoch::from(block_number.0);
    if height > head.epoch() - 1 {
        bail!("requested a future epoch (beyond \"latest\")");
    }
    Ok(chain
        .chain_index()
        .tipset_by_height(height, head, resolve)?)
}

fn resolve_block_hash_tipset<DB: Blockstore>(
    chain: &ChainStore<DB>,
    head: Tipset,
    block_hash: &EthHash,
    require_canonical: bool,
    resolve: ResolveNullTipset,
) -> anyhow::Result<Tipset> {
    let ts = get_tipset_from_hash(chain, block_hash)?;
    // verify that the tipset is in the canonical chain
    if require_canonical {
        // walk up the current chain (our head) until we reach ts.epoch()
        let walk_ts = chain
            .chain_index()
            .tipset_by_height(ts.epoch(), head, resolve)?;
        // verify that it equals the expected tipset
        if walk_ts != ts {
            bail!("tipset is not canonical");
        }
    }
    Ok(ts)
}

fn tipset_by_block_number_or_hash<DB: Blockstore>(
    chain: &ChainStore<DB>,
    block_param: BlockNumberOrHash,
    resolve: ResolveNullTipset,
) -> anyhow::Result<Tipset> {
    let head = chain.heaviest_tipset();
    match block_param {
        BlockNumberOrHash::PredefinedBlock(predefined) => {
            resolve_predefined_tipset(chain, head, predefined)
        }
        BlockNumberOrHash::BlockNumber(block_number)
        | BlockNumberOrHash::BlockNumberObject(BlockNumber { block_number }) => {
            resolve_block_number_tipset(chain, head, block_number, resolve)
        }
        BlockNumberOrHash::BlockHash(block_hash) => {
            resolve_block_hash_tipset(chain, head, &block_hash, false, resolve)
        }
        BlockNumberOrHash::BlockHashObject(BlockHash {
            block_hash,
            require_canonical,
        }) => resolve_block_hash_tipset(chain, head, &block_hash, require_canonical, resolve),
    }
}

async fn tipset_by_block_number_or_hash_v2<DB: Blockstore + Send + Sync + 'static>(
    ctx: &Ctx<DB>,
    block_param: ExtBlockNumberOrHash,
    resolve: ResolveNullTipset,
) -> anyhow::Result<Tipset> {
    let chain = ctx.chain_store();
    let head = chain.heaviest_tipset();
    match block_param {
        ExtBlockNumberOrHash::PredefinedBlock(predefined) => {
            resolve_predefined_tipset_v2(ctx, head, predefined).await
        }
        ExtBlockNumberOrHash::BlockNumber(block_number)
        | ExtBlockNumberOrHash::BlockNumberObject(BlockNumber { block_number }) => {
            resolve_block_number_tipset(chain, head, block_number, resolve)
        }
        ExtBlockNumberOrHash::BlockHash(block_hash) => {
            resolve_block_hash_tipset(chain, head, &block_hash, false, resolve)
        }
        ExtBlockNumberOrHash::BlockHashObject(BlockHash {
            block_hash,
            require_canonical,
        }) => resolve_block_hash_tipset(chain, head, &block_hash, require_canonical, resolve),
    }
}

fn tipset_by_ext_block_number_or_hash<DB: Blockstore>(
    chain: &ChainStore<DB>,
    block_param: ExtBlockNumberOrHash,
    resolve: ResolveNullTipset,
) -> anyhow::Result<Tipset> {
    let head = chain.heaviest_tipset();
    match block_param {
        ExtBlockNumberOrHash::PredefinedBlock(ext_predefined) => {
            resolve_ext_predefined_tipset(chain, head, ext_predefined, resolve)
        }
        ExtBlockNumberOrHash::BlockNumber(block_number)
        | ExtBlockNumberOrHash::BlockNumberObject(BlockNumber { block_number }) => {
            resolve_block_number_tipset(chain, head, block_number, resolve)
        }
        ExtBlockNumberOrHash::BlockHash(block_hash) => {
            resolve_block_hash_tipset(chain, head, &block_hash, false, resolve)
        }
        ExtBlockNumberOrHash::BlockHashObject(BlockHash {
            block_hash,
            require_canonical,
        }) => resolve_block_hash_tipset(chain, head, &block_hash, require_canonical, resolve),
    }
}

async fn execute_tipset<DB: Blockstore + Send + Sync + 'static>(
    data: &Ctx<DB>,
    tipset: &Tipset,
) -> Result<(Cid, Vec<(ChainMessage, Receipt)>)> {
    let msgs = data.chain_store().messages_for_tipset(tipset)?;

    let (state_root, _) = data
        .state_manager
        .tipset_state(tipset, StateLookupPolicy::Enabled)
        .await?;
    let receipts = data.state_manager.tipset_message_receipts(tipset).await?;

    if msgs.len() != receipts.len() {
        bail!("receipts and message array lengths didn't match for tipset: {tipset:?}")
    }

    Ok((
        state_root,
        msgs.into_iter().zip(receipts.into_iter()).collect(),
    ))
}

fn is_eth_address(addr: &VmAddress) -> bool {
    if addr.protocol() != Protocol::Delegated {
        return false;
    }
    let f4_addr: Result<DelegatedAddress, _> = addr.payload().try_into();

    f4_addr.is_ok()
}

/// `eth_tx_from_signed_eth_message` does NOT populate:
/// - `hash`
/// - `block_hash`
/// - `block_number`
/// - `transaction_index`
pub fn eth_tx_from_signed_eth_message(
    smsg: &SignedMessage,
    chain_id: EthChainIdType,
) -> Result<(EthAddress, EthTx)> {
    // The from address is always an f410f address, never an ID or other address.
    let from = smsg.message().from;
    if !is_eth_address(&from) {
        bail!("sender must be an eth account, was {from}");
    }
    // This should be impossible to fail as we've already asserted that we have an
    // Ethereum Address sender...
    let from = EthAddress::from_filecoin_address(&from)?;
    let tx = EthTx::from_signed_message(chain_id, smsg)?;
    Ok((from, tx))
}

/// See <https://docs.soliditylang.org/en/latest/abi-spec.html#function-selector-and-argument-encoding>
/// for ABI specification
fn encode_filecoin_params_as_abi(
    method: MethodNum,
    codec: u64,
    params: &fvm_ipld_encoding::RawBytes,
) -> Result<EthBytes> {
    let mut buffer: Vec<u8> = vec![0x86, 0x8e, 0x10, 0xc4];
    buffer.append(&mut encode_filecoin_returns_as_abi(method, codec, params));
    Ok(EthBytes(buffer))
}

fn encode_filecoin_returns_as_abi(
    exit_code: u64,
    codec: u64,
    data: &fvm_ipld_encoding::RawBytes,
) -> Vec<u8> {
    encode_as_abi_helper(exit_code, codec, data)
}

/// Round to the next multiple of `EVM` word length.
fn round_up_word(value: usize) -> usize {
    value.div_ceil(EVM_WORD_LENGTH) * EVM_WORD_LENGTH
}

/// Format two numbers followed by an arbitrary byte array as solidity ABI.
fn encode_as_abi_helper(param1: u64, param2: u64, data: &[u8]) -> Vec<u8> {
    // The first two params are "static" numbers. Then, we record the offset of the "data" arg,
    // then, at that offset, we record the length of the data.
    //
    // In practice, this means we have 4 256-bit words back to back where the third arg (the
    // offset) is _always_ '32*3'.
    let static_args = [
        param1,
        param2,
        (EVM_WORD_LENGTH * 3) as u64,
        data.len() as u64,
    ];
    let padding = [0u8; 24];
    let buf: Vec<u8> = padding
        .iter() // Right pad
        .chain(static_args[0].to_be_bytes().iter()) // Copy u64
        .chain(padding.iter())
        .chain(static_args[1].to_be_bytes().iter())
        .chain(padding.iter())
        .chain(static_args[2].to_be_bytes().iter())
        .chain(padding.iter())
        .chain(static_args[3].to_be_bytes().iter())
        .chain(data.iter()) // Finally, we copy in the data
        .chain(std::iter::repeat_n(
            &0u8,
            round_up_word(data.len()) - data.len(),
        )) // Left pad
        .cloned()
        .collect();

    buf
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
    chain_id: EthChainIdType,
) -> Result<ApiEthTx> {
    // Lookup the from address. This must succeed.
    let from = match lookup_eth_address(&msg.from(), state) {
        Ok(Some(from)) => from,
        _ => bail!(
            "failed to lookup sender address {} when converting a native message to an eth txn",
            msg.from()
        ),
    };
    // Lookup the to address. If the recipient doesn't exist, we replace the address with a
    // known sentinel address.
    let mut to = match lookup_eth_address(&msg.to(), state) {
        Ok(Some(addr)) => Some(addr),
        Ok(None) => Some(EthAddress(
            ethereum_types::H160::from_str(REVERTED_ETH_ADDRESS).unwrap(),
        )),
        Err(err) => {
            bail!(err)
        }
    };

    // Finally, convert the input parameters to "solidity ABI".

    // For empty, we use "0" as the codec. Otherwise, we use CBOR for message
    // parameters.
    let codec = if !msg.params().is_empty() { CBOR } else { 0 };

    // We try to decode the input as an EVM method invocation and/or a contract creation. If
    // that fails, we encode the "native" parameters as Solidity ABI.
    let input = 'decode: {
        if (msg.method_num() == EVMMethod::InvokeContract as MethodNum
            || msg.method_num() == EAMMethod::CreateExternal as MethodNum)
            && let Ok(buffer) = decode_payload(msg.params(), codec)
        {
            // If this is a valid "create external", unset the "to" address.
            if msg.method_num() == EAMMethod::CreateExternal as MethodNum {
                to = None;
            }
            break 'decode buffer;
        }
        // Yeah, we're going to ignore errors here because the user can send whatever they
        // want and may send garbage.
        encode_filecoin_params_as_abi(msg.method_num(), codec, msg.params())?
    };

    Ok(ApiEthTx {
        to,
        from,
        input,
        nonce: EthUint64(msg.sequence),
        chain_id: EthUint64(chain_id),
        value: msg.value.clone().into(),
        r#type: EthUint64(EIP_1559_TX_TYPE.into()),
        gas: EthUint64(msg.gas_limit),
        max_fee_per_gas: Some(msg.gas_fee_cap.clone().into()),
        max_priority_fee_per_gas: Some(msg.gas_premium.clone().into()),
        access_list: vec![],
        ..ApiEthTx::default()
    })
}

pub fn new_eth_tx_from_signed_message<DB: Blockstore>(
    smsg: &SignedMessage,
    state: &StateTree<DB>,
    chain_id: EthChainIdType,
) -> Result<ApiEthTx> {
    let (tx, hash) = if smsg.is_delegated() {
        // This is an eth tx
        let (from, tx) = eth_tx_from_signed_eth_message(smsg, chain_id)?;
        let hash = tx.eth_hash()?.into();
        let tx = ApiEthTx { from, ..tx.into() };
        (tx, hash)
    } else if smsg.is_secp256k1() {
        // Secp Filecoin Message
        let tx = eth_tx_from_native_message(smsg.message(), state, chain_id)?;
        (tx, smsg.cid().into())
    } else {
        // BLS Filecoin message
        let tx = eth_tx_from_native_message(smsg.message(), state, chain_id)?;
        (tx, smsg.message().cid().into())
    };
    Ok(ApiEthTx { hash, ..tx })
}

/// Creates an Ethereum transaction from Filecoin message lookup. If `None` is passed for `tx_index`,
/// it looks up the transaction index of the message in the tipset.
/// Otherwise, it uses some index passed into the function.
fn new_eth_tx_from_message_lookup<DB: Blockstore>(
    ctx: &Ctx<DB>,
    message_lookup: &MessageLookup,
    tx_index: Option<u64>,
) -> Result<ApiEthTx> {
    let ts = ctx
        .chain_store()
        .load_required_tipset_or_heaviest(&message_lookup.tipset)?;

    // This transaction is located in the parent tipset
    let parent_ts = ctx
        .chain_store()
        .load_required_tipset_or_heaviest(ts.parents())?;

    let parent_ts_cid = parent_ts.key().cid()?;

    // Lookup the transaction index
    let tx_index = tx_index.map_or_else(
        || {
            let msgs = ctx.chain_store().messages_for_tipset(&parent_ts)?;
            msgs.iter()
                .position(|msg| msg.cid() == message_lookup.message)
                .context("cannot find the msg in the tipset")
                .map(|i| i as u64)
        },
        Ok,
    )?;

    let smsg = get_signed_message(ctx, message_lookup.message)?;

    let state = ctx.state_manager.get_state_tree(ts.parent_state())?;

    Ok(ApiEthTx {
        block_hash: parent_ts_cid.into(),
        block_number: (parent_ts.epoch() as u64).into(),
        transaction_index: tx_index.into(),
        ..new_eth_tx_from_signed_message(&smsg, &state, ctx.chain_config().eth_chain_id)?
    })
}

fn new_eth_tx<DB: Blockstore>(
    ctx: &Ctx<DB>,
    state: &StateTree<DB>,
    block_height: ChainEpoch,
    msg_tipset_cid: &Cid,
    msg_cid: &Cid,
    tx_index: u64,
) -> Result<ApiEthTx> {
    let smsg = get_signed_message(ctx, *msg_cid)?;
    let tx = new_eth_tx_from_signed_message(&smsg, state, ctx.chain_config().eth_chain_id)?;

    Ok(ApiEthTx {
        block_hash: (*msg_tipset_cid).into(),
        block_number: (block_height as u64).into(),
        transaction_index: tx_index.into(),
        ..tx
    })
}

async fn new_eth_tx_receipt<DB: Blockstore + Send + Sync + 'static>(
    ctx: &Ctx<DB>,
    tipset: &Tipset,
    tx: &ApiEthTx,
    msg_receipt: &Receipt,
) -> anyhow::Result<EthTxReceipt> {
    let mut tx_receipt = EthTxReceipt {
        transaction_hash: tx.hash,
        from: tx.from,
        to: tx.to,
        transaction_index: tx.transaction_index,
        block_hash: tx.block_hash,
        block_number: tx.block_number,
        r#type: tx.r#type,
        status: (msg_receipt.exit_code().is_success() as u64).into(),
        gas_used: msg_receipt.gas_used().into(),
        ..EthTxReceipt::new()
    };

    tx_receipt.cumulative_gas_used = EthUint64::default();

    let gas_fee_cap = tx.gas_fee_cap()?;
    let gas_premium = tx.gas_premium()?;

    let gas_outputs = GasOutputs::compute(
        msg_receipt.gas_used(),
        tx.gas.into(),
        &tipset.block_headers().first().parent_base_fee,
        &gas_fee_cap.0.into(),
        &gas_premium.0.into(),
    );
    let total_spent: BigInt = gas_outputs.total_spent().into();

    let mut effective_gas_price = EthBigInt::default();
    if msg_receipt.gas_used() > 0 {
        effective_gas_price = (total_spent / msg_receipt.gas_used()).into();
    }
    tx_receipt.effective_gas_price = effective_gas_price;

    if tx_receipt.to.is_none() && msg_receipt.exit_code().is_success() {
        // Create and Create2 return the same things.
        let ret: eam::CreateExternalReturn =
            from_slice_with_fallback(msg_receipt.return_data().bytes())?;

        tx_receipt.contract_address = Some(ret.eth_address.0.into());
    }

    if msg_receipt.events_root().is_some() {
        let logs =
            eth_logs_for_block_and_transaction(ctx, tipset, &tx.block_hash, &tx.hash).await?;
        if !logs.is_empty() {
            tx_receipt.logs = logs;
        }
    }

    let mut bloom = Bloom::default();
    for log in tx_receipt.logs.iter() {
        for topic in log.topics.iter() {
            bloom.accrue(topic.0.as_bytes());
        }
        bloom.accrue(log.address.0.as_bytes());
    }
    tx_receipt.logs_bloom = bloom.into();

    Ok(tx_receipt)
}

pub async fn eth_logs_for_block_and_transaction<DB: Blockstore + Send + Sync + 'static>(
    ctx: &Ctx<DB>,
    ts: &Tipset,
    block_hash: &EthHash,
    tx_hash: &EthHash,
) -> anyhow::Result<Vec<EthLog>> {
    let spec = EthFilterSpec {
        block_hash: Some(*block_hash),
        ..Default::default()
    };

    eth_logs_with_filter(ctx, ts, Some(spec), Some(tx_hash)).await
}

pub async fn eth_logs_with_filter<DB: Blockstore + Send + Sync + 'static>(
    ctx: &Ctx<DB>,
    ts: &Tipset,
    spec: Option<EthFilterSpec>,
    tx_hash: Option<&EthHash>,
) -> anyhow::Result<Vec<EthLog>> {
    let mut events = vec![];
    EthEventHandler::collect_events(
        ctx,
        ts,
        spec.as_ref(),
        SkipEvent::OnUnresolvedAddress,
        &mut events,
    )
    .await?;

    let logs = eth_filter_logs_from_events(ctx, &events)?;
    Ok(match tx_hash {
        Some(hash) => logs
            .into_iter()
            .filter(|log| &log.transaction_hash == hash)
            .collect(),
        None => logs, // no tx hash, keep all logs
    })
}

fn get_signed_message<DB: Blockstore>(ctx: &Ctx<DB>, message_cid: Cid) -> Result<SignedMessage> {
    let result: Result<SignedMessage, crate::chain::Error> =
        crate::chain::message_from_cid(ctx.store(), &message_cid);

    result.or_else(|_| {
        // We couldn't find the signed message, it might be a BLS message, so search for a regular message.
        let msg: Message = crate::chain::message_from_cid(ctx.store(), &message_cid)
            .with_context(|| format!("failed to find msg {message_cid}"))?;
        Ok(SignedMessage::new_unchecked(
            msg,
            Signature::new_bls(vec![]),
        ))
    })
}

pub enum EthGetBlockByHash {}
impl RpcMethod<2> for EthGetBlockByHash {
    const NAME: &'static str = "Filecoin.EthGetBlockByHash";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getBlockByHash");
    const PARAM_NAMES: [&'static str; 2] = ["blockHash", "fullTxInfo"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthHash, bool);
    type Ok = Block;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_hash, full_tx_info): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash(
            ctx.chain_store(),
            BlockNumberOrHash::from_block_hash(block_hash),
            ResolveNullTipset::TakeOlder,
        )?;
        Block::from_filecoin_tipset(ctx, ts, full_tx_info.into())
            .await
            .map_err(ServerError::from)
    }
}

pub enum EthGetBlockByNumber {}
impl RpcMethod<2> for EthGetBlockByNumber {
    const NAME: &'static str = "Filecoin.EthGetBlockByNumber";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getBlockByNumber");
    const PARAM_NAMES: [&'static str; 2] = ["blockParam", "fullTxInfo"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Retrieves a block by its number or a special tag.");

    type Params = (BlockNumberOrPredefined, bool);
    type Ok = Block;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_param, full_tx_info): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_ext_block_number_or_hash(
            ctx.chain_store(),
            block_param.into(),
            ResolveNullTipset::TakeOlder,
        )?;
        Block::from_filecoin_tipset(ctx, ts, full_tx_info.into())
            .await
            .map_err(ServerError::from)
    }
}

pub enum EthGetBlockByNumberV2 {}
impl RpcMethod<2> for EthGetBlockByNumberV2 {
    const NAME: &'static str = "Filecoin.EthGetBlockByNumber";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getBlockByNumber");
    const PARAM_NAMES: [&'static str; 2] = ["blockParam", "fullTxInfo"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V2);
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Retrieves a block by its number or a special tag.");

    type Params = (BlockNumberOrPredefined, bool);
    type Ok = Block;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_param, full_tx_info): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash_v2(
            &ctx,
            block_param.into(),
            ResolveNullTipset::TakeOlder,
        )
        .await?;
        Block::from_filecoin_tipset(ctx, ts, full_tx_info.into())
            .await
            .map_err(ServerError::from)
    }
}

async fn get_block_receipts<DB: Blockstore + Send + Sync + 'static>(
    ctx: &Ctx<DB>,
    ts: Tipset,
    limit: Option<ChainEpoch>,
) -> Result<Vec<EthTxReceipt>> {
    if let Some(limit) = limit
        && limit > LOOKBACK_NO_LIMIT
        && ts.epoch() < ctx.chain_store().heaviest_tipset().epoch() - limit
    {
        bail!(
            "tipset {} is older than the allowed lookback limit",
            ts.key().format_lotus()
        );
    }
    let ts_ref = Arc::new(ts);
    let ts_key = ts_ref.key();

    // Execute the tipset to get the messages and receipts
    let (state_root, msgs_and_receipts) = execute_tipset(ctx, &ts_ref).await?;

    // Load the state tree
    let state_tree = ctx.state_manager.get_state_tree(&state_root)?;

    let mut eth_receipts = Vec::with_capacity(msgs_and_receipts.len());
    for (i, (msg, receipt)) in msgs_and_receipts.into_iter().enumerate() {
        let tx = new_eth_tx(
            ctx,
            &state_tree,
            ts_ref.epoch(),
            &ts_key.cid()?,
            &msg.cid(),
            i as u64,
        )?;

        let receipt = new_eth_tx_receipt(ctx, &ts_ref, &tx, &receipt).await?;
        eth_receipts.push(receipt);
    }
    Ok(eth_receipts)
}

pub enum EthGetBlockReceipts {}
impl RpcMethod<1> for EthGetBlockReceipts {
    const NAME: &'static str = "Filecoin.EthGetBlockReceipts";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getBlockReceipts");
    const PARAM_NAMES: [&'static str; 1] = ["blockParam"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Retrieves all transaction receipts for a block by its number, hash or a special tag.",
    );

    type Params = (BlockNumberOrHash,);
    type Ok = Vec<EthTxReceipt>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_param,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash(
            ctx.chain_store(),
            block_param,
            ResolveNullTipset::TakeOlder,
        )?;
        get_block_receipts(&ctx, ts, None)
            .await
            .map_err(ServerError::from)
    }
}

pub enum EthGetBlockReceiptsV2 {}
impl RpcMethod<1> for EthGetBlockReceiptsV2 {
    const NAME: &'static str = "Filecoin.EthGetBlockReceipts";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getBlockReceipts");
    const PARAM_NAMES: [&'static str; 1] = ["blockParam"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V2);
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Retrieves all transaction receipts for a block by its number, hash or a special tag.",
    );

    type Params = (ExtBlockNumberOrHash,);
    type Ok = Vec<EthTxReceipt>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_param,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash_v2(&ctx, block_param, ResolveNullTipset::TakeOlder)
            .await?;
        get_block_receipts(&ctx, ts, None)
            .await
            .map_err(ServerError::from)
    }
}

pub enum EthGetBlockReceiptsLimited {}
impl RpcMethod<2> for EthGetBlockReceiptsLimited {
    const NAME: &'static str = "Filecoin.EthGetBlockReceiptsLimited";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getBlockReceiptsLimited");
    const PARAM_NAMES: [&'static str; 2] = ["blockParam", "limit"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Retrieves all transaction receipts for a block identified by its number, hash or a special tag along with an optional limit on the chain epoch for state resolution.",
    );

    type Params = (BlockNumberOrHash, ChainEpoch);
    type Ok = Vec<EthTxReceipt>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_param, limit): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash(
            ctx.chain_store(),
            block_param,
            ResolveNullTipset::TakeOlder,
        )?;
        get_block_receipts(&ctx, ts, Some(limit))
            .await
            .map_err(ServerError::from)
    }
}

pub enum EthGetBlockReceiptsLimitedV2 {}
impl RpcMethod<2> for EthGetBlockReceiptsLimitedV2 {
    const NAME: &'static str = "Filecoin.EthGetBlockReceiptsLimited";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getBlockReceiptsLimited");
    const PARAM_NAMES: [&'static str; 2] = ["blockParam", "limit"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V2);
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Retrieves all transaction receipts for a block identified by its number, hash or a special tag along with an optional limit on the chain epoch for state resolution.",
    );

    type Params = (ExtBlockNumberOrHash, ChainEpoch);
    type Ok = Vec<EthTxReceipt>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_param, limit): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash_v2(&ctx, block_param, ResolveNullTipset::TakeOlder)
            .await?;
        get_block_receipts(&ctx, ts, Some(limit))
            .await
            .map_err(ServerError::from)
    }
}

pub enum EthGetBlockTransactionCountByHash {}
impl RpcMethod<1> for EthGetBlockTransactionCountByHash {
    const NAME: &'static str = "Filecoin.EthGetBlockTransactionCountByHash";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getBlockTransactionCountByHash");
    const PARAM_NAMES: [&'static str; 1] = ["blockHash"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthHash,);
    type Ok = EthUint64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_hash,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = get_tipset_from_hash(ctx.chain_store(), &block_hash)?;

        let head = ctx.chain_store().heaviest_tipset();
        if ts.epoch() > head.epoch() {
            return Err(anyhow::anyhow!("requested a future epoch (beyond \"latest\")").into());
        }
        let count = count_messages_in_tipset(ctx.store(), &ts)?;
        Ok(EthUint64(count as _))
    }
}

pub enum EthGetBlockTransactionCountByNumber {}
impl RpcMethod<1> for EthGetBlockTransactionCountByNumber {
    const NAME: &'static str = "Filecoin.EthGetBlockTransactionCountByNumber";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getBlockTransactionCountByNumber");
    const PARAM_NAMES: [&'static str; 1] = ["blockNumber"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the number of transactions in a block identified by its block number.");

    type Params = (EthInt64,);
    type Ok = EthUint64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_number,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let height = block_number.0;
        let head = ctx.chain_store().heaviest_tipset();
        if height > head.epoch() {
            return Err(anyhow::anyhow!("requested a future epoch (beyond \"latest\")").into());
        }
        let ts = ctx
            .chain_index()
            .tipset_by_height(height, head, ResolveNullTipset::TakeOlder)?;
        let count = count_messages_in_tipset(ctx.store(), &ts)?;
        Ok(EthUint64(count as _))
    }
}

pub enum EthGetBlockTransactionCountByNumberV2 {}
impl RpcMethod<1> for EthGetBlockTransactionCountByNumberV2 {
    const NAME: &'static str = "Filecoin.EthGetBlockTransactionCountByNumber";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getBlockTransactionCountByNumber");
    const PARAM_NAMES: [&'static str; 1] = ["blockNumber"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V2);
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Returns the number of transactions in a block identified by its block number or a special tag.",
    );

    type Params = (BlockNumberOrPredefined,);
    type Ok = EthUint64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_number,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash_v2(
            &ctx,
            block_number.into(),
            ResolveNullTipset::TakeOlder,
        )
        .await?;
        let count = count_messages_in_tipset(ctx.store(), &ts)?;
        Ok(EthUint64(count as _))
    }
}

pub enum EthGetMessageCidByTransactionHash {}
impl RpcMethod<1> for EthGetMessageCidByTransactionHash {
    const NAME: &'static str = "Filecoin.EthGetMessageCidByTransactionHash";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getMessageCidByTransactionHash");
    const PARAM_NAMES: [&'static str; 1] = ["txHash"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthHash,);
    type Ok = Option<Cid>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (tx_hash,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let result = ctx.chain_store().get_mapping(&tx_hash);
        match result {
            Ok(Some(cid)) => return Ok(Some(cid)),
            Ok(None) => tracing::debug!("Undefined key {tx_hash}"),
            _ => {
                result?;
            }
        }

        // This isn't an eth transaction we have the mapping for, so let's try looking it up as a filecoin message
        let cid = tx_hash.to_cid();

        let result: Result<Vec<SignedMessage>, crate::chain::Error> =
            crate::chain::messages_from_cids(ctx.store(), &[cid]);
        if result.is_ok() {
            // This is an Eth Tx, Secp message, Or BLS message in the mpool
            return Ok(Some(cid));
        }

        let result: Result<Vec<Message>, crate::chain::Error> =
            crate::chain::messages_from_cids(ctx.store(), &[cid]);
        if result.is_ok() {
            // This is a BLS message
            return Ok(Some(cid));
        }

        // Ethereum clients expect an empty response when the message was not found
        Ok(None)
    }
}

fn count_messages_in_tipset(store: &impl Blockstore, ts: &Tipset) -> anyhow::Result<usize> {
    let mut message_cids = CidHashSet::default();
    for block in ts.block_headers() {
        let (bls_messages, secp_messages) = crate::chain::store::block_messages(store, block)?;
        for m in bls_messages {
            message_cids.insert(m.cid());
        }
        for m in secp_messages {
            message_cids.insert(m.cid());
        }
    }
    Ok(message_cids.len())
}

pub enum EthSyncing {}
impl RpcMethod<0> for EthSyncing {
    const NAME: &'static str = "Filecoin.EthSyncing";
    const NAME_ALIAS: Option<&'static str> = Some("eth_syncing");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = EthSyncingResult;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
        ext: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let sync_status: crate::chain_sync::SyncStatusReport =
            crate::rpc::sync::SyncStatus::handle(ctx, (), ext).await?;
        match sync_status.status {
            NodeSyncStatus::Synced => Ok(EthSyncingResult {
                done_sync: true,
                // Once the node is synced, other fields are not relevant for the API
                ..Default::default()
            }),
            NodeSyncStatus::Syncing => {
                let starting_block = match sync_status.get_min_starting_block() {
                    Some(e) => Ok(e),
                    None => Err(ServerError::internal_error(
                        "missing syncing information, try again",
                        None,
                    )),
                }?;

                Ok(EthSyncingResult {
                    done_sync: sync_status.is_synced(),
                    starting_block,
                    current_block: sync_status.current_head_epoch,
                    highest_block: sync_status.network_head_epoch,
                })
            }
            _ => Err(ServerError::internal_error("node is not syncing", None)),
        }
    }
}

pub enum EthEstimateGas {}

impl RpcMethod<2> for EthEstimateGas {
    const NAME: &'static str = "Filecoin.EthEstimateGas";
    const NAME_ALIAS: Option<&'static str> = Some("eth_estimateGas");
    const N_REQUIRED_PARAMS: usize = 1;
    const PARAM_NAMES: [&'static str; 2] = ["tx", "blockParam"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthCallMessage, Option<BlockNumberOrHash>);
    type Ok = EthUint64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (tx, block_param): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tipset = if let Some(block_param) = block_param {
            tipset_by_block_number_or_hash(
                ctx.chain_store(),
                block_param,
                ResolveNullTipset::TakeOlder,
            )?
        } else {
            ctx.chain_store().heaviest_tipset()
        };
        eth_estimate_gas(&ctx, tx, tipset).await
    }
}

pub enum EthEstimateGasV2 {}

impl RpcMethod<2> for EthEstimateGasV2 {
    const NAME: &'static str = "Filecoin.EthEstimateGas";
    const NAME_ALIAS: Option<&'static str> = Some("eth_estimateGas");
    const N_REQUIRED_PARAMS: usize = 1;
    const PARAM_NAMES: [&'static str; 2] = ["tx", "blockParam"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V2);
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthCallMessage, Option<ExtBlockNumberOrHash>);
    type Ok = EthUint64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (tx, block_param): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tipset = if let Some(block_param) = block_param {
            tipset_by_block_number_or_hash_v2(&ctx, block_param, ResolveNullTipset::TakeOlder)
                .await?
        } else {
            ctx.chain_store().heaviest_tipset()
        };
        eth_estimate_gas(&ctx, tx, tipset).await
    }
}

async fn eth_estimate_gas<DB>(
    ctx: &Ctx<DB>,
    tx: EthCallMessage,
    tipset: Tipset,
) -> Result<EthUint64, ServerError>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let mut msg = Message::try_from(tx)?;
    // Set the gas limit to the zero sentinel value, which makes
    // gas estimation actually run.
    msg.gas_limit = 0;

    match gas::estimate_message_gas(ctx, msg.clone(), None, tipset.key().clone().into()).await {
        Err(mut err) => {
            // On failure, GasEstimateMessageGas doesn't actually return the invocation result,
            // it just returns an error. That means we can't get the revert reason.
            //
            // So we re-execute the message with EthCall (well, applyMessage which contains the
            // guts of EthCall). This will give us an ethereum specific error with revert
            // information.
            msg.set_gas_limit(BLOCK_GAS_LIMIT);
            if let Err(e) = apply_message(ctx, Some(tipset), msg).await {
                // if the error is an execution reverted, return it directly
                if e.downcast_ref::<EthErrors>()
                    .is_some_and(|eth_err| matches!(eth_err, EthErrors::ExecutionReverted { .. }))
                {
                    return Err(e.into());
                }

                err = e.into();
            }

            Err(anyhow::anyhow!("failed to estimate gas: {err}").into())
        }
        Ok(gassed_msg) => {
            let expected_gas = eth_gas_search(ctx, gassed_msg, &tipset.key().into()).await?;
            Ok(expected_gas.into())
        }
    }
}

async fn apply_message<DB>(
    ctx: &Ctx<DB>,
    tipset: Option<Tipset>,
    msg: Message,
) -> Result<ApiInvocResult, Error>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let (invoc_res, _) = ctx
        .state_manager
        .apply_on_state_with_gas(tipset, msg, StateLookupPolicy::Enabled, VMFlush::Skip)
        .await
        .map_err(|e| anyhow::anyhow!("failed to apply on state with gas: {e}"))?;

    // Extract receipt or return early if none
    match &invoc_res.msg_rct {
        None => return Err(anyhow::anyhow!("no message receipt in execution result")),
        Some(receipt) => {
            if !receipt.exit_code().is_success() {
                let (data, reason) = decode_revert_reason(receipt.return_data());

                return Err(EthErrors::execution_reverted(
                    ExitCode::from(receipt.exit_code()),
                    reason.as_str(),
                    invoc_res.error.as_str(),
                    data.as_slice(),
                )
                .into());
            }
        }
    };

    Ok(invoc_res)
}

pub async fn eth_gas_search<DB>(
    data: &Ctx<DB>,
    msg: Message,
    tsk: &ApiTipsetKey,
) -> anyhow::Result<u64>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let (_invoc_res, apply_ret, prior_messages, ts) =
        gas::GasEstimateGasLimit::estimate_call_with_gas(data, msg.clone(), tsk, VMTrace::Traced)
            .await?;
    if apply_ret.msg_receipt().exit_code().is_success() {
        return Ok(msg.gas_limit());
    }

    let exec_trace = apply_ret.exec_trace();
    let _expected_exit_code: ExitCode = fvm_shared4::error::ExitCode::SYS_OUT_OF_GAS.into();
    if exec_trace.iter().any(|t| {
        matches!(
            t,
            &ExecutionEvent::CallReturn(CallReturn {
                exit_code: Some(_expected_exit_code),
                ..
            })
        )
    }) {
        let ret = gas_search(data, &msg, &prior_messages, ts).await?;
        Ok(((ret as f64) * data.mpool.config.gas_limit_overestimation) as u64)
    } else {
        anyhow::bail!(
            "message execution failed: exit {}, reason: {}",
            apply_ret.msg_receipt().exit_code(),
            apply_ret.failure_info().unwrap_or_default(),
        );
    }
}

/// `gas_search` does an exponential search to find a gas value to execute the
/// message with. It first finds a high gas limit that allows the message to execute
/// by doubling the previous gas limit until it succeeds then does a binary
/// search till it gets within a range of 1%
async fn gas_search<DB>(
    data: &Ctx<DB>,
    msg: &Message,
    prior_messages: &[ChainMessage],
    ts: Tipset,
) -> anyhow::Result<u64>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let mut high = msg.gas_limit;
    let mut low = msg.gas_limit;

    async fn can_succeed<DB>(
        data: &Ctx<DB>,
        mut msg: Message,
        prior_messages: &[ChainMessage],
        ts: Tipset,
        limit: u64,
    ) -> anyhow::Result<bool>
    where
        DB: Blockstore + Send + Sync + 'static,
    {
        msg.gas_limit = limit;
        let (_invoc_res, apply_ret, _, _) = data
            .state_manager
            .call_with_gas(
                &mut msg.into(),
                prior_messages,
                Some(ts),
                VMTrace::NotTraced,
                StateLookupPolicy::Enabled,
                VMFlush::Skip,
            )
            .await?;
        Ok(apply_ret.msg_receipt().exit_code().is_success())
    }

    while high < BLOCK_GAS_LIMIT {
        if can_succeed(data, msg.clone(), prior_messages, ts.clone(), high).await? {
            break;
        }
        low = high;
        high = high.saturating_mul(2).min(BLOCK_GAS_LIMIT);
    }

    let mut check_threshold = high / 100;
    while (high - low) > check_threshold {
        let median = (high + low) / 2;
        if can_succeed(data, msg.clone(), prior_messages, ts.clone(), median).await? {
            high = median;
        } else {
            low = median;
        }
        check_threshold = median / 100;
    }

    Ok(high)
}

pub enum EthFeeHistory {}

impl RpcMethod<3> for EthFeeHistory {
    const NAME: &'static str = "Filecoin.EthFeeHistory";
    const NAME_ALIAS: Option<&'static str> = Some("eth_feeHistory");
    const N_REQUIRED_PARAMS: usize = 2;
    const PARAM_NAMES: [&'static str; 3] = ["blockCount", "newestBlockNumber", "rewardPercentiles"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthUint64, BlockNumberOrPredefined, Option<Vec<f64>>);
    type Ok = EthFeeHistoryResult;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (EthUint64(block_count), newest_block_number, reward_percentiles): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tipset = tipset_by_ext_block_number_or_hash(
            ctx.chain_store(),
            newest_block_number.into(),
            ResolveNullTipset::TakeOlder,
        )?;

        eth_fee_history(ctx, tipset, block_count, reward_percentiles).await
    }
}

pub enum EthFeeHistoryV2 {}

impl RpcMethod<3> for EthFeeHistoryV2 {
    const NAME: &'static str = "Filecoin.EthFeeHistory";
    const NAME_ALIAS: Option<&'static str> = Some("eth_feeHistory");
    const N_REQUIRED_PARAMS: usize = 2;
    const PARAM_NAMES: [&'static str; 3] = ["blockCount", "newestBlockNumber", "rewardPercentiles"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V2);
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthUint64, ExtBlockNumberOrHash, Option<Vec<f64>>);
    type Ok = EthFeeHistoryResult;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (EthUint64(block_count), newest_block_number, reward_percentiles): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tipset = tipset_by_block_number_or_hash_v2(
            &ctx,
            newest_block_number,
            ResolveNullTipset::TakeOlder,
        )
        .await?;

        eth_fee_history(ctx, tipset, block_count, reward_percentiles).await
    }
}

async fn eth_fee_history<B: Blockstore + Send + Sync + 'static>(
    ctx: Ctx<B>,
    tipset: Tipset,
    block_count: u64,
    reward_percentiles: Option<Vec<f64>>,
) -> Result<EthFeeHistoryResult, ServerError> {
    if block_count > 1024 {
        return Err(anyhow::anyhow!("block count should be smaller than 1024").into());
    }

    let reward_percentiles = reward_percentiles.unwrap_or_default();
    validate_reward_percentiles(&reward_percentiles)?;

    let mut oldest_block_height = 1;
    // NOTE: baseFeePerGas should include the next block after the newest of the returned range,
    //  because the next base fee can be inferred from the messages in the newest block.
    //  However, this is NOT the case in Filecoin due to deferred execution, so the best
    //  we can do is duplicate the last value.
    let mut base_fee_array = vec![EthBigInt::from(
        &tipset.block_headers().first().parent_base_fee,
    )];
    let mut rewards_array = vec![];
    let mut gas_used_ratio_array = vec![];
    for ts in tipset
        .chain(ctx.store())
        .filter(|i| i.epoch() > 0)
        .take(block_count as _)
    {
        let base_fee = &ts.block_headers().first().parent_base_fee;
        let (_state_root, messages_and_receipts) = execute_tipset(&ctx, &ts).await?;
        let mut tx_gas_rewards = Vec::with_capacity(messages_and_receipts.len());
        for (message, receipt) in messages_and_receipts {
            let premium = message.effective_gas_premium(base_fee);
            tx_gas_rewards.push(GasReward {
                gas_used: receipt.gas_used(),
                premium,
            });
        }
        let (rewards, total_gas_used) =
            calculate_rewards_and_gas_used(&reward_percentiles, tx_gas_rewards);
        let max_gas = BLOCK_GAS_LIMIT * (ts.block_headers().len() as u64);

        // arrays should be reversed at the end
        base_fee_array.push(EthBigInt::from(base_fee));
        gas_used_ratio_array.push((total_gas_used as f64) / (max_gas as f64));
        rewards_array.push(rewards);

        oldest_block_height = ts.epoch();
    }

    // Reverse the arrays; we collected them newest to oldest; the client expects oldest to newest.
    base_fee_array.reverse();
    gas_used_ratio_array.reverse();
    rewards_array.reverse();

    Ok(EthFeeHistoryResult {
        oldest_block: EthUint64(oldest_block_height as _),
        base_fee_per_gas: base_fee_array,
        gas_used_ratio: gas_used_ratio_array,
        reward: if reward_percentiles.is_empty() {
            None
        } else {
            Some(rewards_array)
        },
    })
}

fn validate_reward_percentiles(reward_percentiles: &[f64]) -> anyhow::Result<()> {
    if reward_percentiles.len() > 100 {
        anyhow::bail!("length of the reward percentile array cannot be greater than 100");
    }

    for (&rp_prev, &rp) in std::iter::once(&0.0)
        .chain(reward_percentiles.iter())
        .tuple_windows()
    {
        if !(0. ..=100.).contains(&rp) {
            anyhow::bail!("invalid reward percentile: {rp} should be between 0 and 100");
        }
        if rp < rp_prev {
            anyhow::bail!(
                "invalid reward percentile: {rp} should be larger than or equal to {rp_prev}"
            );
        }
    }

    Ok(())
}

fn calculate_rewards_and_gas_used(
    reward_percentiles: &[f64],
    mut tx_gas_rewards: Vec<GasReward>,
) -> (Vec<EthBigInt>, u64) {
    const MIN_GAS_PREMIUM: u64 = 100000;

    let gas_used_total = tx_gas_rewards.iter().map(|i| i.gas_used).sum();
    let mut rewards = reward_percentiles
        .iter()
        .map(|_| EthBigInt(MIN_GAS_PREMIUM.into()))
        .collect_vec();
    if !tx_gas_rewards.is_empty() {
        tx_gas_rewards.sort_by_key(|i| i.premium.clone());
        let mut idx = 0;
        let mut sum = 0;
        #[allow(clippy::indexing_slicing)]
        for (i, &percentile) in reward_percentiles.iter().enumerate() {
            let threshold = ((gas_used_total as f64) * percentile / 100.) as u64;
            while sum < threshold && idx < tx_gas_rewards.len() - 1 {
                sum += tx_gas_rewards[idx].gas_used;
                idx += 1;
            }
            rewards[i] = (&tx_gas_rewards[idx].premium).into();
        }
    }
    (rewards, gas_used_total)
}

pub enum EthGetCode {}
impl RpcMethod<2> for EthGetCode {
    const NAME: &'static str = "Filecoin.EthGetCode";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getCode");
    const PARAM_NAMES: [&'static str; 2] = ["ethAddress", "blockNumberOrHash"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Retrieves the contract code at a specific address and block state, identified by its number, hash, or a special tag.",
    );

    type Params = (EthAddress, BlockNumberOrHash);
    type Ok = EthBytes;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (eth_address, block_param): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash(
            ctx.chain_store(),
            block_param,
            ResolveNullTipset::TakeOlder,
        )?;
        eth_get_code(&ctx, &ts, &eth_address).await
    }
}

pub enum EthGetCodeV2 {}
impl RpcMethod<2> for EthGetCodeV2 {
    const NAME: &'static str = "Filecoin.EthGetCode";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getCode");
    const PARAM_NAMES: [&'static str; 2] = ["ethAddress", "blockNumberOrHash"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V2);
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Retrieves the contract code at a specific address and block state, identified by its number, hash, or a special tag.",
    );

    type Params = (EthAddress, ExtBlockNumberOrHash);
    type Ok = EthBytes;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (eth_address, block_param): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash_v2(&ctx, block_param, ResolveNullTipset::TakeOlder)
            .await?;
        eth_get_code(&ctx, &ts, &eth_address).await
    }
}

async fn eth_get_code<DB>(
    ctx: &Ctx<DB>,
    ts: &Tipset,
    eth_address: &EthAddress,
) -> Result<EthBytes, ServerError>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let to_address = FilecoinAddress::try_from(eth_address)?;
    let (state, _) = ctx
        .state_manager
        .tipset_state(ts, StateLookupPolicy::Enabled)
        .await?;
    let state_tree = ctx.state_manager.get_state_tree(&state)?;
    let Some(actor) = state_tree
        .get_actor(&to_address)
        .with_context(|| format!("failed to lookup contract {}", eth_address.0))?
    else {
        return Ok(Default::default());
    };

    // Not a contract. We could try to distinguish between accounts and "native" contracts here,
    // but it's not worth it.
    if !is_evm_actor(&actor.code) {
        return Ok(Default::default());
    }

    let message = Message {
        from: FilecoinAddress::SYSTEM_ACTOR,
        to: to_address,
        method_num: METHOD_GET_BYTE_CODE,
        gas_limit: BLOCK_GAS_LIMIT,
        ..Default::default()
    };

    let api_invoc_result = 'invoc: {
        for ts in ts.clone().chain(ctx.store()) {
            match ctx.state_manager.call_on_state(state, &message, Some(ts)) {
                Ok(res) => {
                    break 'invoc res;
                }
                Err(e) => tracing::warn!(%e),
            }
        }
        return Err(anyhow::anyhow!("Call failed").into());
    };
    let Some(msg_rct) = api_invoc_result.msg_rct else {
        return Err(anyhow::anyhow!("no message receipt").into());
    };
    if !msg_rct.exit_code().is_success() || !api_invoc_result.error.is_empty() {
        return Err(anyhow::anyhow!(
            "GetBytecode failed: exit={} error={}",
            msg_rct.exit_code(),
            api_invoc_result.error
        )
        .into());
    }

    let get_bytecode_return: GetBytecodeReturn =
        fvm_ipld_encoding::from_slice(msg_rct.return_data().as_slice())?;
    if let Some(cid) = get_bytecode_return.0 {
        Ok(EthBytes(ctx.store().get_required(&cid)?))
    } else {
        Ok(Default::default())
    }
}

pub enum EthGetStorageAt {}
impl RpcMethod<3> for EthGetStorageAt {
    const NAME: &'static str = "Filecoin.EthGetStorageAt";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getStorageAt");
    const PARAM_NAMES: [&'static str; 3] = ["ethAddress", "position", "blockNumberOrHash"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Retrieves the storage value at a specific position for a contract
        at a given block state, identified by its number, hash, or a special tag.",
    );

    type Params = (EthAddress, EthBytes, BlockNumberOrHash);
    type Ok = EthBytes;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (eth_address, position, block_number_or_hash): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash(
            ctx.chain_store(),
            block_number_or_hash,
            ResolveNullTipset::TakeOlder,
        )?;
        get_storage_at(&ctx, ts, eth_address, position).await
    }
}

pub enum EthGetStorageAtV2 {}
impl RpcMethod<3> for EthGetStorageAtV2 {
    const NAME: &'static str = "Filecoin.EthGetStorageAt";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getStorageAt");
    const PARAM_NAMES: [&'static str; 3] = ["ethAddress", "position", "blockNumberOrHash"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V2);
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Retrieves the storage value at a specific position for a contract
        at a given block state, identified by its number, hash, or a special tag.",
    );

    type Params = (EthAddress, EthBytes, ExtBlockNumberOrHash);
    type Ok = EthBytes;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (eth_address, position, block_number_or_hash): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash_v2(
            &ctx,
            block_number_or_hash,
            ResolveNullTipset::TakeOlder,
        )
        .await?;
        get_storage_at(&ctx, ts, eth_address, position).await
    }
}

async fn get_storage_at<DB: Blockstore + Send + Sync + 'static>(
    ctx: &Ctx<DB>,
    ts: Tipset,
    eth_address: EthAddress,
    position: EthBytes,
) -> Result<EthBytes, ServerError> {
    let to_address = FilecoinAddress::try_from(&eth_address)?;
    let (state, _) = ctx
        .state_manager
        .tipset_state(&ts, StateLookupPolicy::Enabled)
        .await?;
    let make_empty_result = || EthBytes(vec![0; EVM_WORD_LENGTH]);
    let Some(actor) = ctx
        .state_manager
        .get_actor(&to_address, state)
        .with_context(|| format!("failed to lookup contract {}", eth_address.0))?
    else {
        return Ok(make_empty_result());
    };

    if !is_evm_actor(&actor.code) {
        return Ok(make_empty_result());
    }

    let params = RawBytes::new(GetStorageAtParams::new(position.0)?.serialize_params()?);
    let message = Message {
        from: FilecoinAddress::SYSTEM_ACTOR,
        to: to_address,
        method_num: METHOD_GET_STORAGE_AT,
        gas_limit: BLOCK_GAS_LIMIT,
        params,
        ..Default::default()
    };
    let api_invoc_result = 'invoc: {
        for ts in ts.chain(ctx.store()) {
            match ctx.state_manager.call_on_state(state, &message, Some(ts)) {
                Ok(res) => {
                    break 'invoc res;
                }
                Err(e) => tracing::warn!(%e),
            }
        }
        return Err(anyhow::anyhow!("Call failed").into());
    };
    let Some(msg_rct) = api_invoc_result.msg_rct else {
        return Err(anyhow::anyhow!("no message receipt").into());
    };
    if !msg_rct.exit_code().is_success() || !api_invoc_result.error.is_empty() {
        return Err(
            anyhow::anyhow!("failed to lookup storage slot: {}", api_invoc_result.error).into(),
        );
    }

    let mut ret = fvm_ipld_encoding::from_slice::<RawBytes>(msg_rct.return_data().as_slice())?
        .bytes()
        .to_vec();
    if ret.len() < EVM_WORD_LENGTH {
        let mut with_padding = vec![0; EVM_WORD_LENGTH.saturating_sub(ret.len())];
        with_padding.append(&mut ret);
        Ok(EthBytes(with_padding))
    } else {
        Ok(EthBytes(ret))
    }
}

pub enum EthGetTransactionCount {}
impl RpcMethod<2> for EthGetTransactionCount {
    const NAME: &'static str = "Filecoin.EthGetTransactionCount";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getTransactionCount");
    const PARAM_NAMES: [&'static str; 2] = ["sender", "blockParam"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthAddress, BlockNumberOrHash);
    type Ok = EthUint64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (sender, block_param): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let addr = sender.to_filecoin_address()?;
        match block_param {
            BlockNumberOrHash::PredefinedBlock(Predefined::Pending) => {
                Ok(EthUint64(ctx.mpool.get_sequence(&addr)?))
            }
            _ => {
                let ts = tipset_by_block_number_or_hash(
                    ctx.chain_store(),
                    block_param,
                    ResolveNullTipset::TakeOlder,
                )?;
                eth_get_transaction_count(&ctx, &ts, addr).await
            }
        }
    }
}

pub enum EthGetTransactionCountV2 {}
impl RpcMethod<2> for EthGetTransactionCountV2 {
    const NAME: &'static str = "Filecoin.EthGetTransactionCount";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getTransactionCount");
    const PARAM_NAMES: [&'static str; 2] = ["sender", "blockParam"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V2);
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthAddress, ExtBlockNumberOrHash);
    type Ok = EthUint64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (sender, block_param): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let addr = sender.to_filecoin_address()?;
        match block_param {
            ExtBlockNumberOrHash::PredefinedBlock(ExtPredefined::Pending) => {
                Ok(EthUint64(ctx.mpool.get_sequence(&addr)?))
            }
            _ => {
                let ts = tipset_by_block_number_or_hash_v2(
                    &ctx,
                    block_param,
                    ResolveNullTipset::TakeOlder,
                )
                .await?;
                eth_get_transaction_count(&ctx, &ts, addr).await
            }
        }
    }
}

async fn eth_get_transaction_count<B>(
    ctx: &Ctx<B>,
    ts: &Tipset,
    addr: FilecoinAddress,
) -> Result<EthUint64, ServerError>
where
    B: Blockstore + Send + Sync + 'static,
{
    let (state_cid, _) = ctx
        .state_manager
        .tipset_state(ts, StateLookupPolicy::Enabled)
        .await?;

    let state_tree = ctx.state_manager.get_state_tree(&state_cid)?;
    let actor = match state_tree.get_actor(&addr)? {
        Some(actor) => actor,
        None => return Ok(EthUint64(0)),
    };

    if is_evm_actor(&actor.code) {
        let evm_state = evm::State::load(ctx.store(), actor.code, actor.state)?;
        if !evm_state.is_alive() {
            return Ok(EthUint64(0));
        }
        Ok(EthUint64(evm_state.nonce()))
    } else {
        Ok(EthUint64(actor.sequence))
    }
}

pub enum EthMaxPriorityFeePerGas {}
impl RpcMethod<0> for EthMaxPriorityFeePerGas {
    const NAME: &'static str = "Filecoin.EthMaxPriorityFeePerGas";
    const NAME_ALIAS: Option<&'static str> = Some("eth_maxPriorityFeePerGas");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = EthBigInt;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        match gas::estimate_gas_premium(&ctx, 0, &ApiTipsetKey(None)).await {
            Ok(gas_premium) => Ok(EthBigInt(gas_premium.atto().clone())),
            Err(_) => Ok(EthBigInt(num_bigint::BigInt::zero())),
        }
    }
}

pub enum EthProtocolVersion {}
impl RpcMethod<0> for EthProtocolVersion {
    const NAME: &'static str = "Filecoin.EthProtocolVersion";
    const NAME_ALIAS: Option<&'static str> = Some("eth_protocolVersion");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = EthUint64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let epoch = ctx.chain_store().heaviest_tipset().epoch();
        let version = u32::from(ctx.state_manager.get_network_version(epoch).0);
        Ok(EthUint64(version.into()))
    }
}

pub enum EthGetTransactionByBlockNumberAndIndex {}
impl RpcMethod<2> for EthGetTransactionByBlockNumberAndIndex {
    const NAME: &'static str = "Filecoin.EthGetTransactionByBlockNumberAndIndex";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getTransactionByBlockNumberAndIndex");
    const PARAM_NAMES: [&'static str; 2] = ["blockParam", "txIndex"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Retrieves a transaction by its block number and index.");

    type Params = (BlockNumberOrPredefined, EthUint64);
    type Ok = Option<ApiEthTx>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_param, tx_index): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_ext_block_number_or_hash(
            ctx.chain_store(),
            block_param.into(),
            ResolveNullTipset::TakeOlder,
        )?;
        eth_tx_by_block_num_and_idx(&ctx, &ts, tx_index)
    }
}

pub enum EthGetTransactionByBlockNumberAndIndexV2 {}
impl RpcMethod<2> for EthGetTransactionByBlockNumberAndIndexV2 {
    const NAME: &'static str = "Filecoin.EthGetTransactionByBlockNumberAndIndex";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getTransactionByBlockNumberAndIndex");
    const PARAM_NAMES: [&'static str; 2] = ["blockParam", "txIndex"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V2);
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Retrieves a transaction by its block number and index.");

    type Params = (BlockNumberOrPredefined, EthUint64);
    type Ok = Option<ApiEthTx>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_param, tx_index): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash_v2(
            &ctx,
            block_param.into(),
            ResolveNullTipset::TakeOlder,
        )
        .await?;
        eth_tx_by_block_num_and_idx(&ctx, &ts, tx_index)
    }
}

fn eth_tx_by_block_num_and_idx<B>(
    ctx: &Ctx<B>,
    ts: &Tipset,
    tx_index: EthUint64,
) -> Result<Option<ApiEthTx>, ServerError>
where
    B: Blockstore + Send + Sync + 'static,
{
    let messages = ctx.chain_store().messages_for_tipset(ts)?;

    let EthUint64(index) = tx_index;
    let msg = messages.get(index as usize).with_context(|| {
            format!(
                "failed to get transaction at index {}: index {} out of range: tipset contains {} messages",
                index,
                index,
                messages.len()
            )
        })?;

    let state = ctx.state_manager.get_state_tree(ts.parent_state())?;

    let tx = new_eth_tx(ctx, &state, ts.epoch(), &ts.key().cid()?, &msg.cid(), index)?;

    Ok(Some(tx))
}

pub enum EthGetTransactionByBlockHashAndIndex {}
impl RpcMethod<2> for EthGetTransactionByBlockHashAndIndex {
    const NAME: &'static str = "Filecoin.EthGetTransactionByBlockHashAndIndex";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getTransactionByBlockHashAndIndex");
    const PARAM_NAMES: [&'static str; 2] = ["blockHash", "txIndex"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthHash, EthUint64);
    type Ok = Option<ApiEthTx>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_hash, tx_index): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = get_tipset_from_hash(ctx.chain_store(), &block_hash)?;

        let messages = ctx.chain_store().messages_for_tipset(&ts)?;

        let EthUint64(index) = tx_index;
        let msg = messages.get(index as usize).with_context(|| {
            format!(
                "index {} out of range: tipset contains {} messages",
                index,
                messages.len()
            )
        })?;

        let state = ctx.state_manager.get_state_tree(ts.parent_state())?;

        let tx = new_eth_tx(
            &ctx,
            &state,
            ts.epoch(),
            &ts.key().cid()?,
            &msg.cid(),
            index,
        )?;

        Ok(Some(tx))
    }
}

pub enum EthGetTransactionByHash {}
impl RpcMethod<1> for EthGetTransactionByHash {
    const NAME: &'static str = "Filecoin.EthGetTransactionByHash";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getTransactionByHash");
    const PARAM_NAMES: [&'static str; 1] = ["txHash"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthHash,);
    type Ok = Option<ApiEthTx>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (tx_hash,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        get_eth_transaction_by_hash(&ctx, &tx_hash, None).await
    }
}

pub enum EthGetTransactionByHashLimited {}
impl RpcMethod<2> for EthGetTransactionByHashLimited {
    const NAME: &'static str = "Filecoin.EthGetTransactionByHashLimited";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getTransactionByHashLimited");
    const PARAM_NAMES: [&'static str; 2] = ["txHash", "limit"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthHash, ChainEpoch);
    type Ok = Option<ApiEthTx>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (tx_hash, limit): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        get_eth_transaction_by_hash(&ctx, &tx_hash, Some(limit)).await
    }
}

async fn get_eth_transaction_by_hash(
    ctx: &Ctx<impl Blockstore + Send + Sync + 'static>,
    tx_hash: &EthHash,
    limit: Option<ChainEpoch>,
) -> Result<Option<ApiEthTx>, ServerError> {
    let message_cid = ctx.chain_store().get_mapping(tx_hash)?.unwrap_or_else(|| {
        tracing::debug!(
            "could not find transaction hash {} in Ethereum mapping",
            tx_hash
        );
        // This isn't an eth transaction we have the mapping for, so let's look it up as a filecoin message
        tx_hash.to_cid()
    });

    // First, try to get the cid from mined transactions
    if let Ok(Some((tipset, receipt))) = ctx
        .state_manager
        .search_for_message(None, message_cid, limit, Some(true))
        .await
    {
        let ipld = receipt.return_data().deserialize().unwrap_or(Ipld::Null);
        let message_lookup = MessageLookup {
            receipt,
            tipset: tipset.key().clone(),
            height: tipset.epoch(),
            message: message_cid,
            return_dec: ipld,
        };

        if let Ok(tx) = new_eth_tx_from_message_lookup(ctx, &message_lookup, None) {
            return Ok(Some(tx));
        }
    }

    // If not found, try to get it from the mempool
    let (pending, _) = ctx.mpool.pending()?;

    if let Some(smsg) = pending.iter().find(|item| item.cid() == message_cid) {
        // We only return pending eth-account messages because we can't guarantee
        // that the from/to addresses of other messages are conversable to 0x-style
        // addresses. So we just ignore them.
        //
        // This should be "fine" as anyone using an "Ethereum-centric" block
        // explorer shouldn't care about seeing pending messages from native
        // accounts.
        if let Ok(eth_tx) = EthTx::from_signed_message(ctx.chain_config().eth_chain_id, smsg) {
            return Ok(Some(eth_tx.into()));
        }
    }

    // Ethereum clients expect an empty response when the message was not found
    Ok(None)
}

pub enum EthGetTransactionHashByCid {}
impl RpcMethod<1> for EthGetTransactionHashByCid {
    const NAME: &'static str = "Filecoin.EthGetTransactionHashByCid";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getTransactionHashByCid");
    const PARAM_NAMES: [&'static str; 1] = ["cid"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = (Cid,);
    type Ok = Option<EthHash>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (cid,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let smsgs_result: Result<Vec<SignedMessage>, crate::chain::Error> =
            crate::chain::messages_from_cids(ctx.store(), &[cid]);
        if let Ok(smsgs) = smsgs_result
            && let Some(smsg) = smsgs.first()
        {
            let hash = if smsg.is_delegated() {
                let chain_id = ctx.chain_config().eth_chain_id;
                let (_, tx) = eth_tx_from_signed_eth_message(smsg, chain_id)?;
                tx.eth_hash()?.into()
            } else if smsg.is_secp256k1() {
                smsg.cid().into()
            } else {
                smsg.message().cid().into()
            };
            return Ok(Some(hash));
        }

        let msg_result = crate::chain::get_chain_message(ctx.store(), &cid);
        if let Ok(msg) = msg_result {
            return Ok(Some(msg.cid().into()));
        }

        Ok(None)
    }
}

pub enum EthCall {}
impl RpcMethod<2> for EthCall {
    const NAME: &'static str = "Filecoin.EthCall";
    const NAME_ALIAS: Option<&'static str> = Some("eth_call");
    const N_REQUIRED_PARAMS: usize = 2;
    const PARAM_NAMES: [&'static str; 2] = ["tx", "blockParam"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    type Params = (EthCallMessage, BlockNumberOrHash);
    type Ok = EthBytes;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (tx, block_param): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash(
            ctx.chain_store(),
            block_param,
            ResolveNullTipset::TakeOlder,
        )?;
        eth_call(&ctx, tx, ts).await
    }
}

pub enum EthCallV2 {}
impl RpcMethod<2> for EthCallV2 {
    const NAME: &'static str = "Filecoin.EthCall";
    const NAME_ALIAS: Option<&'static str> = Some("eth_call");
    const N_REQUIRED_PARAMS: usize = 2;
    const PARAM_NAMES: [&'static str; 2] = ["tx", "blockParam"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V2);
    const PERMISSION: Permission = Permission::Read;
    type Params = (EthCallMessage, ExtBlockNumberOrHash);
    type Ok = EthBytes;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (tx, block_param): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash_v2(&ctx, block_param, ResolveNullTipset::TakeOlder)
            .await?;
        eth_call(&ctx, tx, ts).await
    }
}

async fn eth_call<DB>(
    ctx: &Ctx<DB>,
    tx: EthCallMessage,
    ts: Tipset,
) -> Result<EthBytes, ServerError>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let msg = Message::try_from(tx)?;
    let invoke_result = apply_message(ctx, Some(ts), msg.clone()).await?;

    if msg.to() == FilecoinAddress::ETHEREUM_ACCOUNT_MANAGER_ACTOR {
        Ok(EthBytes::default())
    } else {
        let msg_rct = invoke_result.msg_rct.context("no message receipt")?;
        let return_data = msg_rct.return_data();
        if return_data.is_empty() {
            Ok(Default::default())
        } else {
            let bytes = decode_payload(&return_data, CBOR)?;
            Ok(bytes)
        }
    }
}

pub enum EthNewFilter {}
impl RpcMethod<1> for EthNewFilter {
    const NAME: &'static str = "Filecoin.EthNewFilter";
    const NAME_ALIAS: Option<&'static str> = Some("eth_newFilter");
    const PARAM_NAMES: [&'static str; 1] = ["filterSpec"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthFilterSpec,);
    type Ok = FilterID;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (filter_spec,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let eth_event_handler = ctx.eth_event_handler.clone();
        let chain_height = ctx.chain_store().heaviest_tipset().epoch();
        Ok(eth_event_handler.eth_new_filter(&filter_spec, chain_height)?)
    }
}

pub enum EthNewPendingTransactionFilter {}
impl RpcMethod<0> for EthNewPendingTransactionFilter {
    const NAME: &'static str = "Filecoin.EthNewPendingTransactionFilter";
    const NAME_ALIAS: Option<&'static str> = Some("eth_newPendingTransactionFilter");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = FilterID;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let eth_event_handler = ctx.eth_event_handler.clone();

        Ok(eth_event_handler.eth_new_pending_transaction_filter()?)
    }
}

pub enum EthNewBlockFilter {}
impl RpcMethod<0> for EthNewBlockFilter {
    const NAME: &'static str = "Filecoin.EthNewBlockFilter";
    const NAME_ALIAS: Option<&'static str> = Some("eth_newBlockFilter");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = FilterID;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let eth_event_handler = ctx.eth_event_handler.clone();

        Ok(eth_event_handler.eth_new_block_filter()?)
    }
}

pub enum EthUninstallFilter {}
impl RpcMethod<1> for EthUninstallFilter {
    const NAME: &'static str = "Filecoin.EthUninstallFilter";
    const NAME_ALIAS: Option<&'static str> = Some("eth_uninstallFilter");
    const PARAM_NAMES: [&'static str; 1] = ["filterId"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = (FilterID,);
    type Ok = bool;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (filter_id,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let eth_event_handler = ctx.eth_event_handler.clone();

        Ok(eth_event_handler.eth_uninstall_filter(&filter_id)?)
    }
}

pub enum EthUnsubscribe {}
impl RpcMethod<0> for EthUnsubscribe {
    const NAME: &'static str = "Filecoin.EthUnsubscribe";
    const NAME_ALIAS: Option<&'static str> = Some("eth_unsubscribe");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;
    const SUBSCRIPTION: bool = true;

    type Params = ();
    type Ok = ();

    // This method is a placeholder and is never actually called.
    // Subscription handling is performed in [`pubsub.rs`](pubsub).
    //
    // We still need to implement the [`RpcMethod`] trait to expose method metadata
    // like [`NAME`](Self::NAME), [`NAME_ALIAS`](Self::NAME_ALIAS), [`PERMISSION`](Self::PERMISSION), etc..
    async fn handle(
        _: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(())
    }
}

pub enum EthSubscribe {}
impl RpcMethod<0> for EthSubscribe {
    const NAME: &'static str = "Filecoin.EthSubscribe";
    const NAME_ALIAS: Option<&'static str> = Some("eth_subscribe");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;
    const SUBSCRIPTION: bool = true;

    type Params = ();
    type Ok = ();

    // This method is a placeholder and is never actually called.
    // Subscription handling is performed in [`pubsub.rs`](pubsub).
    //
    // We still need to implement the [`RpcMethod`] trait to expose method metadata
    // like [`NAME`](Self::NAME), [`NAME_ALIAS`](Self::NAME_ALIAS), [`PERMISSION`](Self::PERMISSION), etc..
    async fn handle(
        _: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(())
    }
}

pub enum EthAddressToFilecoinAddress {}
impl RpcMethod<1> for EthAddressToFilecoinAddress {
    const NAME: &'static str = "Filecoin.EthAddressToFilecoinAddress";
    const PARAM_NAMES: [&'static str; 1] = ["ethAddress"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Converts an EthAddress into an f410 Filecoin Address");
    type Params = (EthAddress,);
    type Ok = FilecoinAddress;
    async fn handle(
        _ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (eth_address,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(eth_address.to_filecoin_address()?)
    }
}

pub enum FilecoinAddressToEthAddress {}
impl RpcMethod<2> for FilecoinAddressToEthAddress {
    const NAME: &'static str = "Filecoin.FilecoinAddressToEthAddress";
    const N_REQUIRED_PARAMS: usize = 1;
    const PARAM_NAMES: [&'static str; 2] = ["filecoinAddress", "blockParam"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Converts any Filecoin address to an EthAddress");
    type Params = (FilecoinAddress, Option<BlockNumberOrPredefined>);
    type Ok = EthAddress;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, block_param): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        if let Ok(eth_address) = EthAddress::from_filecoin_address(&address) {
            Ok(eth_address)
        } else {
            let block_param = block_param.unwrap_or(BlockNumberOrPredefined::PredefinedBlock(
                ExtPredefined::Finalized,
            ));
            let ts = tipset_by_ext_block_number_or_hash(
                ctx.chain_store(),
                block_param.into(),
                ResolveNullTipset::TakeOlder,
            )?;

            let id_address = ctx.state_manager.lookup_required_id(&address, &ts)?;
            Ok(EthAddress::from_filecoin_address(&id_address)?)
        }
    }
}

async fn get_eth_transaction_receipt(
    ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
    tx_hash: EthHash,
    limit: Option<ChainEpoch>,
) -> Result<Option<EthTxReceipt>, ServerError> {
    let msg_cid = ctx.chain_store().get_mapping(&tx_hash)?.unwrap_or_else(|| {
        tracing::debug!(
            "could not find transaction hash {} in Ethereum mapping",
            tx_hash
        );
        // This isn't an eth transaction we have the mapping for, so let's look it up as a filecoin message
        tx_hash.to_cid()
    });

    let option = ctx
        .state_manager
        .search_for_message(None, msg_cid, limit, Some(true))
        .await
        .with_context(|| format!("failed to lookup Eth Txn {tx_hash} as {msg_cid}"));

    let option = match option {
        Ok(opt) => opt,
        // Ethereum clients expect an empty response when the message was not found
        Err(e) => {
            tracing::debug!("could not find transaction receipt for hash {tx_hash}: {e}");
            return Ok(None);
        }
    };

    let (tipset, receipt) = option.context("not indexed")?;
    let ipld = receipt.return_data().deserialize().unwrap_or(Ipld::Null);
    let message_lookup = MessageLookup {
        receipt,
        tipset: tipset.key().clone(),
        height: tipset.epoch(),
        message: msg_cid,
        return_dec: ipld,
    };

    let tx = new_eth_tx_from_message_lookup(&ctx, &message_lookup, None)
        .with_context(|| format!("failed to convert {tx_hash} into an Eth Tx"))?;

    let ts = ctx
        .chain_index()
        .load_required_tipset(&message_lookup.tipset)?;

    // The tx is located in the parent tipset
    let parent_ts = ctx
        .chain_index()
        .load_required_tipset(ts.parents())
        .map_err(|e| {
            format!(
                "failed to lookup tipset {} when constructing the eth txn receipt: {}",
                ts.parents(),
                e
            )
        })?;

    let tx_receipt = new_eth_tx_receipt(&ctx, &parent_ts, &tx, &message_lookup.receipt).await?;

    Ok(Some(tx_receipt))
}

pub enum EthGetTransactionReceipt {}
impl RpcMethod<1> for EthGetTransactionReceipt {
    const NAME: &'static str = "Filecoin.EthGetTransactionReceipt";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getTransactionReceipt");
    const N_REQUIRED_PARAMS: usize = 1;
    const PARAM_NAMES: [&'static str; 1] = ["txHash"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;
    type Params = (EthHash,);
    type Ok = Option<EthTxReceipt>;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (tx_hash,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        get_eth_transaction_receipt(ctx, tx_hash, None).await
    }
}

pub enum EthGetTransactionReceiptLimited {}
impl RpcMethod<2> for EthGetTransactionReceiptLimited {
    const NAME: &'static str = "Filecoin.EthGetTransactionReceiptLimited";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getTransactionReceiptLimited");
    const N_REQUIRED_PARAMS: usize = 1;
    const PARAM_NAMES: [&'static str; 2] = ["txHash", "limit"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;
    type Params = (EthHash, ChainEpoch);
    type Ok = Option<EthTxReceipt>;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (tx_hash, limit): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        get_eth_transaction_receipt(ctx, tx_hash, Some(limit)).await
    }
}

pub enum EthSendRawTransaction {}
impl RpcMethod<1> for EthSendRawTransaction {
    const NAME: &'static str = "Filecoin.EthSendRawTransaction";
    const NAME_ALIAS: Option<&'static str> = Some("eth_sendRawTransaction");
    const PARAM_NAMES: [&'static str; 1] = ["rawTx"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthBytes,);
    type Ok = EthHash;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (raw_tx,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tx_args = parse_eth_transaction(&raw_tx.0)?;
        let smsg = tx_args.get_signed_message(ctx.chain_config().eth_chain_id)?;
        let cid = ctx.mpool.as_ref().push(smsg).await?;
        Ok(cid.into())
    }
}

pub enum EthSendRawTransactionUntrusted {}
impl RpcMethod<1> for EthSendRawTransactionUntrusted {
    const NAME: &'static str = "Filecoin.EthSendRawTransactionUntrusted";
    const NAME_ALIAS: Option<&'static str> = Some("eth_sendRawTransactionUntrusted");
    const PARAM_NAMES: [&'static str; 1] = ["rawTx"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthBytes,);
    type Ok = EthHash;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (raw_tx,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tx_args = parse_eth_transaction(&raw_tx.0)?;
        let smsg = tx_args.get_signed_message(ctx.chain_config().eth_chain_id)?;
        let cid = ctx.mpool.as_ref().push_untrusted(smsg).await?;
        Ok(cid.into())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CollectedEvent {
    pub(crate) entries: Vec<EventEntry>,
    pub(crate) emitter_addr: crate::shim::address::Address,
    pub(crate) event_idx: u64,
    pub(crate) reverted: bool,
    pub(crate) height: ChainEpoch,
    pub(crate) tipset_key: TipsetKey,
    msg_idx: u64,
    pub(crate) msg_cid: Cid,
}

fn match_key(key: &str) -> Option<usize> {
    match key.get(0..2) {
        Some("t1") => Some(0),
        Some("t2") => Some(1),
        Some("t3") => Some(2),
        Some("t4") => Some(3),
        _ => None,
    }
}

fn eth_log_from_event(entries: &[EventEntry]) -> Option<(EthBytes, Vec<EthHash>)> {
    let mut topics_found = [false; 4];
    let mut topics_found_count = 0;
    let mut data_found = false;
    let mut data: EthBytes = EthBytes::default();
    let mut topics: Vec<EthHash> = Vec::default();
    for entry in entries {
        // Drop events with non-raw topics. Built-in actors emit CBOR, and anything else would be
        // invalid anyway.
        if entry.codec != IPLD_RAW {
            return None;
        }
        // Check if the key is t1..t4
        if let Some(idx) = match_key(&entry.key) {
            // Drop events with mis-sized topics.
            let result: Result<[u8; EVM_WORD_LENGTH], _> = entry.value.0.clone().try_into();
            let bytes = if let Ok(value) = result {
                value
            } else {
                tracing::warn!(
                    "got an EVM event topic with an invalid size (key: {}, size: {})",
                    entry.key,
                    entry.value.0.len()
                );
                return None;
            };
            // Drop events with duplicate topics.
            if *topics_found.get(idx).expect("Infallible") {
                tracing::warn!("got a duplicate EVM event topic (key: {})", entry.key);
                return None;
            }
            *topics_found.get_mut(idx).expect("Infallible") = true;
            topics_found_count += 1;
            // Extend the topics array
            if topics.len() <= idx {
                topics.resize(idx + 1, EthHash::default());
            }
            *topics.get_mut(idx).expect("Infallible") = bytes.into();
        } else if entry.key == "d" {
            // Drop events with duplicate data fields.
            if data_found {
                tracing::warn!("got duplicate EVM event data");
                return None;
            }
            data_found = true;
            data = EthBytes(entry.value.0.clone());
        } else {
            // Skip entries we don't understand (makes it easier to extend things).
            // But we warn for now because we don't expect them.
            tracing::warn!("unexpected event entry (key: {})", entry.key);
        }
    }
    // Drop events with skipped topics.
    if topics.len() != topics_found_count {
        tracing::warn!(
            "EVM event topic length mismatch (expected: {}, actual: {})",
            topics.len(),
            topics_found_count
        );
        return None;
    }
    Some((data, topics))
}

fn eth_tx_hash_from_signed_message(
    message: &SignedMessage,
    eth_chain_id: EthChainIdType,
) -> anyhow::Result<EthHash> {
    if message.is_delegated() {
        let (_, tx) = eth_tx_from_signed_eth_message(message, eth_chain_id)?;
        Ok(tx.eth_hash()?.into())
    } else if message.is_secp256k1() {
        Ok(message.cid().into())
    } else {
        Ok(message.message().cid().into())
    }
}

fn eth_tx_hash_from_message_cid<DB: Blockstore>(
    blockstore: &DB,
    message_cid: &Cid,
    eth_chain_id: EthChainIdType,
) -> anyhow::Result<Option<EthHash>> {
    if let Ok(smsg) = crate::chain::message_from_cid(blockstore, message_cid) {
        // This is an Eth Tx, Secp message, Or BLS message in the mpool
        return Ok(Some(eth_tx_hash_from_signed_message(&smsg, eth_chain_id)?));
    }
    let result: Result<Message, _> = crate::chain::message_from_cid(blockstore, message_cid);
    if result.is_ok() {
        // This is a BLS message
        let hash: EthHash = (*message_cid).into();
        return Ok(Some(hash));
    }
    Ok(None)
}

fn transform_events<F>(events: &[CollectedEvent], f: F) -> anyhow::Result<Vec<EthLog>>
where
    F: Fn(&CollectedEvent) -> anyhow::Result<Option<EthLog>>,
{
    events
        .iter()
        .filter_map(|event| match f(event) {
            Ok(Some(eth_log)) => Some(Ok(eth_log)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        })
        .collect()
}

fn eth_filter_logs_from_tipsets(events: &[CollectedEvent]) -> anyhow::Result<Vec<EthHash>> {
    events
        .iter()
        .map(|event| event.tipset_key.cid().map(Into::into))
        .collect()
}

fn eth_filter_logs_from_messages<DB: Blockstore>(
    ctx: &Ctx<DB>,
    events: &[CollectedEvent],
) -> anyhow::Result<Vec<EthHash>> {
    events
        .iter()
        .filter_map(|event| {
            match eth_tx_hash_from_message_cid(
                ctx.store(),
                &event.msg_cid,
                ctx.state_manager.chain_config().eth_chain_id,
            ) {
                Ok(Some(hash)) => Some(Ok(hash)),
                Ok(None) => {
                    tracing::warn!("Ignoring event");
                    None
                }
                Err(err) => Some(Err(err)),
            }
        })
        .collect()
}

fn eth_filter_logs_from_events<DB: Blockstore>(
    ctx: &Ctx<DB>,
    events: &[CollectedEvent],
) -> anyhow::Result<Vec<EthLog>> {
    transform_events(events, |event| {
        let (data, topics) = if let Some((data, topics)) = eth_log_from_event(&event.entries) {
            (data, topics)
        } else {
            tracing::warn!("Ignoring event");
            return Ok(None);
        };
        let transaction_hash = if let Some(transaction_hash) = eth_tx_hash_from_message_cid(
            ctx.store(),
            &event.msg_cid,
            ctx.state_manager.chain_config().eth_chain_id,
        )? {
            transaction_hash
        } else {
            tracing::warn!("Ignoring event");
            return Ok(None);
        };
        let address = EthAddress::from_filecoin_address(&event.emitter_addr)?;
        Ok(Some(EthLog {
            address,
            data,
            topics,
            removed: event.reverted,
            log_index: event.event_idx.into(),
            transaction_index: event.msg_idx.into(),
            transaction_hash,
            block_hash: event.tipset_key.cid()?.into(),
            block_number: (event.height as u64).into(),
        }))
    })
}

fn eth_filter_result_from_events<DB: Blockstore>(
    ctx: &Ctx<DB>,
    events: &[CollectedEvent],
) -> anyhow::Result<EthFilterResult> {
    Ok(EthFilterResult::Logs(eth_filter_logs_from_events(
        ctx, events,
    )?))
}

fn eth_filter_result_from_tipsets(events: &[CollectedEvent]) -> anyhow::Result<EthFilterResult> {
    Ok(EthFilterResult::Hashes(eth_filter_logs_from_tipsets(
        events,
    )?))
}

fn eth_filter_result_from_messages<DB: Blockstore>(
    ctx: &Ctx<DB>,
    events: &[CollectedEvent],
) -> anyhow::Result<EthFilterResult> {
    Ok(EthFilterResult::Hashes(eth_filter_logs_from_messages(
        ctx, events,
    )?))
}

pub enum EthGetLogs {}
impl RpcMethod<1> for EthGetLogs {
    const NAME: &'static str = "Filecoin.EthGetLogs";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getLogs");
    const N_REQUIRED_PARAMS: usize = 1;
    const PARAM_NAMES: [&'static str; 1] = ["ethFilter"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;
    type Params = (EthFilterSpec,);
    type Ok = EthFilterResult;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (eth_filter,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let pf = ctx
            .eth_event_handler
            .parse_eth_filter_spec(&ctx, &eth_filter)?;
        let events = ctx
            .eth_event_handler
            .get_events_for_parsed_filter(&ctx, &pf, SkipEvent::OnUnresolvedAddress)
            .await?;
        Ok(eth_filter_result_from_events(&ctx, &events)?)
    }
}

pub enum EthGetFilterLogs {}
impl RpcMethod<1> for EthGetFilterLogs {
    const NAME: &'static str = "Filecoin.EthGetFilterLogs";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getFilterLogs");
    const N_REQUIRED_PARAMS: usize = 1;
    const PARAM_NAMES: [&'static str; 1] = ["filterId"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Write;
    type Params = (FilterID,);
    type Ok = EthFilterResult;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (filter_id,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let eth_event_handler = ctx.eth_event_handler.clone();
        if let Some(store) = &eth_event_handler.filter_store {
            let filter = store.get(&filter_id)?;
            if let Some(event_filter) = filter.as_any().downcast_ref::<EventFilter>() {
                let events = ctx
                    .eth_event_handler
                    .get_events_for_parsed_filter(
                        &ctx,
                        &event_filter.into(),
                        SkipEvent::OnUnresolvedAddress,
                    )
                    .await?;
                let recent_events: Vec<CollectedEvent> = events
                    .clone()
                    .into_iter()
                    .filter(|event| !event_filter.collected.contains(event))
                    .collect();
                let filter = Arc::new(EventFilter {
                    id: event_filter.id.clone(),
                    tipsets: event_filter.tipsets.clone(),
                    addresses: event_filter.addresses.clone(),
                    keys_with_codec: event_filter.keys_with_codec.clone(),
                    max_results: event_filter.max_results,
                    collected: events.clone(),
                });
                store.update(filter);
                return Ok(eth_filter_result_from_events(&ctx, &recent_events)?);
            }
        }
        Err(anyhow::anyhow!("method not supported").into())
    }
}

pub enum EthGetFilterChanges {}
impl RpcMethod<1> for EthGetFilterChanges {
    const NAME: &'static str = "Filecoin.EthGetFilterChanges";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getFilterChanges");
    const N_REQUIRED_PARAMS: usize = 1;
    const PARAM_NAMES: [&'static str; 1] = ["filterId"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Write;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns event logs which occurred since the last poll");

    type Params = (FilterID,);
    type Ok = EthFilterResult;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (filter_id,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let eth_event_handler = ctx.eth_event_handler.clone();
        if let Some(store) = &eth_event_handler.filter_store {
            let filter = store.get(&filter_id)?;
            if let Some(event_filter) = filter.as_any().downcast_ref::<EventFilter>() {
                let events = ctx
                    .eth_event_handler
                    .get_events_for_parsed_filter(
                        &ctx,
                        &event_filter.into(),
                        SkipEvent::OnUnresolvedAddress,
                    )
                    .await?;
                let recent_events: Vec<CollectedEvent> = events
                    .clone()
                    .into_iter()
                    .filter(|event| !event_filter.collected.contains(event))
                    .collect();
                let filter = Arc::new(EventFilter {
                    id: event_filter.id.clone(),
                    tipsets: event_filter.tipsets.clone(),
                    addresses: event_filter.addresses.clone(),
                    keys_with_codec: event_filter.keys_with_codec.clone(),
                    max_results: event_filter.max_results,
                    collected: events.clone(),
                });
                store.update(filter);
                return Ok(eth_filter_result_from_events(&ctx, &recent_events)?);
            }
            if let Some(tipset_filter) = filter.as_any().downcast_ref::<TipSetFilter>() {
                let events = ctx
                    .eth_event_handler
                    .get_events_for_parsed_filter(
                        &ctx,
                        &ParsedFilter::new_with_tipset(ParsedFilterTipsets::Range(
                            // heaviest tipset doesn't have events because its messages haven't been executed yet
                            RangeInclusive::new(
                                tipset_filter
                                    .collected
                                    .unwrap_or(ctx.chain_store().heaviest_tipset().epoch() - 1),
                                // Use -1 to indicate that the range extends until the latest available tipset.
                                -1,
                            ),
                        )),
                        SkipEvent::OnUnresolvedAddress,
                    )
                    .await?;
                let new_collected = events
                    .iter()
                    .max_by_key(|event| event.height)
                    .map(|e| e.height);
                if let Some(height) = new_collected {
                    let filter = Arc::new(TipSetFilter {
                        id: tipset_filter.id.clone(),
                        max_results: tipset_filter.max_results,
                        collected: Some(height),
                    });
                    store.update(filter);
                }
                return Ok(eth_filter_result_from_tipsets(&events)?);
            }
            if let Some(mempool_filter) = filter.as_any().downcast_ref::<MempoolFilter>() {
                let events = ctx
                    .eth_event_handler
                    .get_events_for_parsed_filter(
                        &ctx,
                        &ParsedFilter::new_with_tipset(ParsedFilterTipsets::Range(
                            // heaviest tipset doesn't have events because its messages haven't been executed yet
                            RangeInclusive::new(
                                mempool_filter
                                    .collected
                                    .unwrap_or(ctx.chain_store().heaviest_tipset().epoch() - 1),
                                // Use -1 to indicate that the range extends until the latest available tipset.
                                -1,
                            ),
                        )),
                        SkipEvent::OnUnresolvedAddress,
                    )
                    .await?;
                let new_collected = events
                    .iter()
                    .max_by_key(|event| event.height)
                    .map(|e| e.height);
                if let Some(height) = new_collected {
                    let filter = Arc::new(MempoolFilter {
                        id: mempool_filter.id.clone(),
                        max_results: mempool_filter.max_results,
                        collected: Some(height),
                    });
                    store.update(filter);
                }
                return Ok(eth_filter_result_from_messages(&ctx, &events)?);
            }
        }
        Err(anyhow::anyhow!("method not supported").into())
    }
}

pub enum EthTraceBlock {}
impl RpcMethod<1> for EthTraceBlock {
    const NAME: &'static str = "Filecoin.EthTraceBlock";
    const NAME_ALIAS: Option<&'static str> = Some("trace_block");
    const N_REQUIRED_PARAMS: usize = 1;
    const PARAM_NAMES: [&'static str; 1] = ["blockParam"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns traces created at given block.");

    type Params = (ExtBlockNumberOrHash,);
    type Ok = Vec<EthBlockTrace>;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_param,): Self::Params,
        ext: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_ext_block_number_or_hash(
            ctx.chain_store(),
            block_param,
            ResolveNullTipset::TakeOlder,
        )?;
        eth_trace_block(&ctx, &ts, ext).await
    }
}

pub enum EthTraceBlockV2 {}
impl RpcMethod<1> for EthTraceBlockV2 {
    const NAME: &'static str = "Filecoin.EthTraceBlock";
    const NAME_ALIAS: Option<&'static str> = Some("trace_block");
    const N_REQUIRED_PARAMS: usize = 1;
    const PARAM_NAMES: [&'static str; 1] = ["blockParam"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V2);
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns traces created at given block.");

    type Params = (ExtBlockNumberOrHash,);
    type Ok = Vec<EthBlockTrace>;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_param,): Self::Params,
        ext: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash_v2(&ctx, block_param, ResolveNullTipset::TakeOlder)
            .await?;
        eth_trace_block(&ctx, &ts, ext).await
    }
}

async fn eth_trace_block<DB>(
    ctx: &Ctx<DB>,
    ts: &Tipset,
    ext: &http::Extensions,
) -> Result<Vec<EthBlockTrace>, ServerError>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let (state_root, trace) = ctx.state_manager.execution_trace(ts)?;
    let state = ctx.state_manager.get_state_tree(&state_root)?;
    let cid = ts.key().cid()?;
    let block_hash: EthHash = cid.into();
    let mut all_traces = vec![];
    let mut msg_idx = 0;
    for ir in trace.into_iter() {
        // ignore messages from system actor
        if ir.msg.from == system::ADDRESS.into() {
            continue;
        }
        msg_idx += 1;
        let tx_hash = EthGetTransactionHashByCid::handle(ctx.clone(), (ir.msg_cid,), ext).await?;
        let tx_hash = tx_hash
            .with_context(|| format!("cannot find transaction hash for cid {}", ir.msg_cid))?;
        let mut env = trace::base_environment(&state, &ir.msg.from)
            .map_err(|e| format!("when processing message {}: {}", ir.msg_cid, e))?;
        if let Some(execution_trace) = ir.execution_trace {
            trace::build_traces(&mut env, &[], execution_trace)?;
            for trace in env.traces {
                all_traces.push(EthBlockTrace {
                    trace: EthTrace {
                        r#type: trace.r#type,
                        subtraces: trace.subtraces,
                        trace_address: trace.trace_address,
                        action: trace.action,
                        result: trace.result,
                        error: trace.error,
                    },
                    block_hash,
                    block_number: ts.epoch(),
                    transaction_hash: tx_hash,
                    transaction_position: msg_idx as i64,
                });
            }
        }
    }
    Ok(all_traces)
}

pub enum EthTraceCall {}
impl RpcMethod<3> for EthTraceCall {
    const NAME: &'static str = "Forest.EthTraceCall";
    const NAME_ALIAS: Option<&'static str> = Some("trace_call");
    const N_REQUIRED_PARAMS: usize = 1;
    const PARAM_NAMES: [&'static str; 3] = ["tx", "traceTypes", "blockParam"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::{ V1 | V2 });
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns parity style trace results for the given transaction.");

    type Params = (
        EthCallMessage,
        NonEmpty<EthTraceType>,
        Option<BlockNumberOrHash>,
    );
    type Ok = EthTraceResults;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (tx, trace_types, block_param): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let msg = Message::try_from(tx)?;
        let block_param = block_param.unwrap_or(BlockNumberOrHash::from_str("latest")?);
        let ts = tipset_by_block_number_or_hash(
            ctx.chain_store(),
            block_param,
            ResolveNullTipset::TakeOlder,
        )?;

        let (pre_state_root, _) = ctx
            .state_manager
            .tipset_state(&ts, StateLookupPolicy::Enabled)
            .await
            .map_err(|e| anyhow::anyhow!("failed to get tipset state: {e}"))?;
        let pre_state = StateTree::new_from_root(ctx.store_owned(), &pre_state_root)?;

        let (invoke_result, post_state_root) = ctx
            .state_manager
            .apply_on_state_with_gas(
                Some(ts.clone()),
                msg.clone(),
                StateLookupPolicy::Enabled,
                VMFlush::Flush,
            )
            .await
            .map_err(|e| anyhow::anyhow!("failed to apply message: {e}"))?;
        let post_state_root =
            post_state_root.context("post-execution state root required for trace call")?;
        let post_state = StateTree::new_from_root(ctx.store_owned(), &post_state_root)?;

        let mut trace_results = EthTraceResults {
            output: get_trace_output(&msg, &invoke_result)?,
            ..Default::default()
        };

        // Extract touched addresses for state diff (do this before consuming exec_trace)
        let touched_addresses = invoke_result
            .execution_trace
            .as_ref()
            .map(extract_touched_eth_addresses)
            .unwrap_or_default();

        // Build call traces if requested
        if trace_types.contains(&EthTraceType::Trace)
            && let Some(exec_trace) = invoke_result.execution_trace
        {
            let mut env = trace::base_environment(&post_state, &msg.from())
                .map_err(|e| anyhow::anyhow!("failed to create trace environment: {e}"))?;
            trace::build_traces(&mut env, &[], exec_trace)?;
            trace_results.trace = env.traces;
        }

        // Build state diff if requested
        if trace_types.contains(&EthTraceType::StateDiff) {
            // Add the caller address to touched addresses
            let mut all_touched = touched_addresses;
            if let Ok(caller_eth) = EthAddress::from_filecoin_address(&msg.from()) {
                all_touched.insert(caller_eth);
            }
            if let Ok(to_eth) = EthAddress::from_filecoin_address(&msg.to()) {
                all_touched.insert(to_eth);
            }

            let state_diff =
                trace::build_state_diff(ctx.store(), &pre_state, &post_state, &all_touched)?;
            trace_results.state_diff = Some(state_diff);
        }

        Ok(trace_results)
    }
}

/// Get output bytes from trace execution result.
fn get_trace_output(msg: &Message, invoke_result: &ApiInvocResult) -> Result<EthBytes> {
    if msg.to() == FilecoinAddress::ETHEREUM_ACCOUNT_MANAGER_ACTOR {
        return Ok(EthBytes::default());
    }

    let msg_rct = invoke_result
        .msg_rct
        .as_ref()
        .context("missing message receipt")?;
    let return_data = msg_rct.return_data();

    if return_data.is_empty() {
        return Ok(EthBytes::default());
    }

    match decode_payload(&return_data, CBOR) {
        Ok(payload) => Ok(payload),
        Err(e) => Err(anyhow::anyhow!("failed to decode return data: {e}")),
    }
}

/// Extract all unique Ethereum addresses touched during execution from the trace.
fn extract_touched_eth_addresses(trace: &crate::rpc::state::ExecutionTrace) -> HashSet<EthAddress> {
    let mut addresses = HashSet::default();
    let mut stack = vec![trace];

    while let Some(current) = stack.pop() {
        if let Ok(eth_addr) = EthAddress::from_filecoin_address(&current.msg.from) {
            addresses.insert(eth_addr);
        }
        if let Ok(eth_addr) = EthAddress::from_filecoin_address(&current.msg.to) {
            addresses.insert(eth_addr);
        }
        stack.extend(&current.subcalls);
    }

    addresses
}

pub enum EthTraceTransaction {}
impl RpcMethod<1> for EthTraceTransaction {
    const NAME: &'static str = "Filecoin.EthTraceTransaction";
    const NAME_ALIAS: Option<&'static str> = Some("trace_transaction");
    const N_REQUIRED_PARAMS: usize = 1;
    const PARAM_NAMES: [&'static str; 1] = ["txHash"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the traces for a specific transaction.");

    type Params = (String,);
    type Ok = Vec<EthBlockTrace>;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (tx_hash,): Self::Params,
        ext: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let eth_hash = EthHash::from_str(&tx_hash).context("invalid transaction hash")?;
        let eth_txn = get_eth_transaction_by_hash(&ctx, &eth_hash, None)
            .await?
            .ok_or(ServerError::internal_error("transaction not found", None))?;

        let ts = tipset_by_ext_block_number_or_hash(
            ctx.chain_store(),
            ExtBlockNumberOrHash::from_block_number(eth_txn.block_number.0 as i64),
            ResolveNullTipset::TakeOlder,
        )?;

        let traces = eth_trace_block(&ctx, &ts, ext)
            .await?
            .into_iter()
            .filter(|trace| trace.transaction_hash == eth_hash)
            .collect();
        Ok(traces)
    }
}

pub enum EthTraceReplayBlockTransactions {}
impl RpcMethod<2> for EthTraceReplayBlockTransactions {
    const N_REQUIRED_PARAMS: usize = 2;
    const NAME: &'static str = "Filecoin.EthTraceReplayBlockTransactions";
    const NAME_ALIAS: Option<&'static str> = Some("trace_replayBlockTransactions");
    const PARAM_NAMES: [&'static str; 2] = ["blockParam", "traceTypes"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Replays all transactions in a block returning the requested traces for each transaction.",
    );

    type Params = (ExtBlockNumberOrHash, Vec<String>);
    type Ok = Vec<EthReplayBlockTransactionTrace>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_param, trace_types): Self::Params,
        ext: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        if trace_types.as_slice() != ["trace"] {
            return Err(ServerError::invalid_params(
                "only 'trace' is supported",
                None,
            ));
        }

        let ts = tipset_by_ext_block_number_or_hash(
            ctx.chain_store(),
            block_param,
            ResolveNullTipset::TakeOlder,
        )?;

        eth_trace_replay_block_transactions(&ctx, &ts, ext).await
    }
}

pub enum EthTraceReplayBlockTransactionsV2 {}
impl RpcMethod<2> for EthTraceReplayBlockTransactionsV2 {
    const N_REQUIRED_PARAMS: usize = 2;
    const NAME: &'static str = "Filecoin.EthTraceReplayBlockTransactions";
    const NAME_ALIAS: Option<&'static str> = Some("trace_replayBlockTransactions");
    const PARAM_NAMES: [&'static str; 2] = ["blockParam", "traceTypes"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V2);
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Replays all transactions in a block returning the requested traces for each transaction.",
    );

    type Params = (ExtBlockNumberOrHash, Vec<String>);
    type Ok = Vec<EthReplayBlockTransactionTrace>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_param, trace_types): Self::Params,
        ext: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        if trace_types.as_slice() != ["trace"] {
            return Err(ServerError::invalid_params(
                "only 'trace' is supported",
                None,
            ));
        }

        let ts = tipset_by_block_number_or_hash_v2(&ctx, block_param, ResolveNullTipset::TakeOlder)
            .await?;

        eth_trace_replay_block_transactions(&ctx, &ts, ext).await
    }
}

async fn eth_trace_replay_block_transactions<DB>(
    ctx: &Ctx<DB>,
    ts: &Tipset,
    ext: &http::Extensions,
) -> Result<Vec<EthReplayBlockTransactionTrace>, ServerError>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let (state_root, trace) = ctx.state_manager.execution_trace(ts)?;

    let state = ctx.state_manager.get_state_tree(&state_root)?;

    let mut all_traces = vec![];
    for ir in trace.into_iter() {
        if ir.msg.from == system::ADDRESS.into() {
            continue;
        }

        let tx_hash = EthGetTransactionHashByCid::handle(ctx.clone(), (ir.msg_cid,), ext).await?;
        let tx_hash = tx_hash
            .with_context(|| format!("cannot find transaction hash for cid {}", ir.msg_cid))?;

        let mut env = trace::base_environment(&state, &ir.msg.from)
            .map_err(|e| format!("when processing message {}: {}", ir.msg_cid, e))?;

        if let Some(execution_trace) = ir.execution_trace {
            trace::build_traces(&mut env, &[], execution_trace)?;

            let get_output = || -> EthBytes {
                env.traces
                    .first()
                    .map_or_else(EthBytes::default, |trace| match &trace.result {
                        TraceResult::Call(r) => r.output.clone(),
                        TraceResult::Create(r) => r.code.clone(),
                    })
            };

            all_traces.push(EthReplayBlockTransactionTrace {
                output: get_output(),
                state_diff: None,
                trace: env.traces.clone(),
                transaction_hash: tx_hash,
                vm_trace: None,
            });
        };
    }

    Ok(all_traces)
}

fn get_eth_block_number_from_string<DB: Blockstore>(
    chain_store: &ChainStore<DB>,
    block: Option<&str>,
    resolve: ResolveNullTipset,
) -> Result<EthUint64> {
    let block_param = match block {
        Some(block_str) => ExtBlockNumberOrHash::from_str(block_str)?,
        None => bail!("cannot parse fromBlock"),
    };
    Ok(EthUint64(
        tipset_by_ext_block_number_or_hash(chain_store, block_param, resolve)?.epoch() as u64,
    ))
}

pub enum EthTraceFilter {}
impl RpcMethod<1> for EthTraceFilter {
    const N_REQUIRED_PARAMS: usize = 1;
    const NAME: &'static str = "Filecoin.EthTraceFilter";
    const NAME_ALIAS: Option<&'static str> = Some("trace_filter");
    const PARAM_NAMES: [&'static str; 1] = ["filter"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the traces for transactions matching the filter criteria.");
    type Params = (EthTraceFilterCriteria,);
    type Ok = Vec<EthBlockTrace>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (filter,): Self::Params,
        ext: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let from_block = get_eth_block_number_from_string(
            ctx.chain_store(),
            filter.from_block.as_deref(),
            ResolveNullTipset::TakeNewer,
        )
        .context("cannot parse fromBlock")?;

        let to_block = get_eth_block_number_from_string(
            ctx.chain_store(),
            filter.to_block.as_deref(),
            ResolveNullTipset::TakeOlder,
        )
        .context("cannot parse toBlock")?;

        Ok(trace_filter(ctx, filter, from_block, to_block, ext)
            .await?
            .into_iter()
            .sorted_by_key(|trace| {
                (
                    trace.block_number,
                    trace.transaction_position,
                    trace.trace.trace_address.clone(),
                )
            })
            .collect_vec())
    }
}

async fn trace_filter(
    ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
    filter: EthTraceFilterCriteria,
    from_block: EthUint64,
    to_block: EthUint64,
    ext: &http::Extensions,
) -> Result<HashSet<EthBlockTrace>> {
    let mut results = HashSet::default();
    if let Some(EthUint64(0)) = filter.count {
        return Ok(results);
    }
    let count = *filter.count.unwrap_or_default();
    ensure!(
        count <= *FOREST_TRACE_FILTER_MAX_RESULT,
        "invalid response count, requested {}, maximum supported is {}",
        count,
        *FOREST_TRACE_FILTER_MAX_RESULT
    );

    let mut trace_counter = 0;
    for blk_num in from_block.0..=to_block.0 {
        let block_traces = EthTraceBlock::handle(
            ctx.clone(),
            (ExtBlockNumberOrHash::from_block_number(blk_num as i64),),
            ext,
        )
        .await?;
        for block_trace in block_traces {
            if block_trace
                .trace
                .match_filter_criteria(&filter.from_address, &filter.to_address)?
            {
                trace_counter += 1;
                if let Some(after) = filter.after
                    && trace_counter <= after.0
                {
                    continue;
                }

                results.insert(block_trace);

                if filter.count.is_some() && results.len() >= count as usize {
                    return Ok(results);
                } else if results.len() > *FOREST_TRACE_FILTER_MAX_RESULT as usize {
                    bail!(
                        "too many results, maximum supported is {}, try paginating requests with After and Count",
                        *FOREST_TRACE_FILTER_MAX_RESULT
                    );
                }
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::rpc::eth::EventEntry;
    use crate::rpc::state::{ExecutionTrace, MessageTrace, ReturnTrace};
    use crate::shim::{econ::TokenAmount, error::ExitCode};
    use crate::{
        db::MemoryDB,
        test_utils::{construct_bls_messages, construct_eth_messages, construct_messages},
    };
    use fvm_shared4::event::Flags;
    use quickcheck::Arbitrary;
    use quickcheck_macros::quickcheck;

    impl Arbitrary for EthHash {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let arr: [u8; 32] = std::array::from_fn(|_ix| u8::arbitrary(g));
            Self(ethereum_types::H256(arr))
        }
    }

    #[quickcheck]
    fn gas_price_result_serde_roundtrip(i: u128) {
        let r = EthBigInt(i.into());
        let encoded = serde_json::to_string(&r).unwrap();
        assert_eq!(encoded, format!("\"{i:#x}\""));
        let decoded: EthBigInt = serde_json::from_str(&encoded).unwrap();
        assert_eq!(r.0, decoded.0);
    }

    #[test]
    fn test_abi_encoding() {
        const EXPECTED: &str = "000000000000000000000000000000000000000000000000000000000000001600000000000000000000000000000000000000000000000000000000000000510000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000001b1111111111111111111020200301000000044444444444444444010000000000";
        const DATA: &str = "111111111111111111102020030100000004444444444444444401";
        let expected_bytes = hex::decode(EXPECTED).unwrap();
        let data_bytes = hex::decode(DATA).unwrap();

        assert_eq!(expected_bytes, encode_as_abi_helper(22, 81, &data_bytes));
    }

    #[test]
    fn test_abi_encoding_empty_bytes() {
        // Generated using https://abi.hashex.org/
        const EXPECTED: &str = "0000000000000000000000000000000000000000000000000000000000000016000000000000000000000000000000000000000000000000000000000000005100000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000000";
        let expected_bytes = hex::decode(EXPECTED).unwrap();
        let data_bytes = vec![];

        assert_eq!(expected_bytes, encode_as_abi_helper(22, 81, &data_bytes));
    }

    #[test]
    fn test_abi_encoding_one_byte() {
        // According to https://docs.soliditylang.org/en/latest/abi-spec.html and handcrafted
        // Uint64, Uint64, Bytes[]
        // 22, 81, [253]
        const EXPECTED: &str = "0000000000000000000000000000000000000000000000000000000000000016000000000000000000000000000000000000000000000000000000000000005100000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000001fd00000000000000000000000000000000000000000000000000000000000000";
        let expected_bytes = hex::decode(EXPECTED).unwrap();
        let data_bytes = vec![253];

        assert_eq!(expected_bytes, encode_as_abi_helper(22, 81, &data_bytes));
    }

    #[test]
    fn test_id_address_roundtrip() {
        let test_cases = [1u64, 2, 3, 100, 101];

        for id in test_cases {
            let addr = FilecoinAddress::new_id(id);

            // roundtrip
            let eth_addr = EthAddress::from_filecoin_address(&addr).unwrap();
            let fil_addr = eth_addr.to_filecoin_address().unwrap();
            assert_eq!(addr, fil_addr)
        }
    }

    #[test]
    fn test_addr_serde_roundtrip() {
        let test_cases = [
            r#""0xd4c5fb16488Aa48081296299d54b0c648C9333dA""#,
            r#""0x2C2EC67e3e1FeA8e4A39601cB3A3Cd44f5fa830d""#,
            r#""0x01184F793982104363F9a8a5845743f452dE0586""#,
        ];

        for addr in test_cases {
            let eth_addr: EthAddress = serde_json::from_str(addr).unwrap();

            let encoded = serde_json::to_string(&eth_addr).unwrap();
            assert_eq!(encoded, addr.to_lowercase());

            let decoded: EthAddress = serde_json::from_str(&encoded).unwrap();
            assert_eq!(eth_addr, decoded);
        }
    }

    #[quickcheck]
    fn test_fil_address_roundtrip(addr: FilecoinAddress) {
        if let Ok(eth_addr) = EthAddress::from_filecoin_address(&addr) {
            let fil_addr = eth_addr.to_filecoin_address().unwrap();

            let protocol = addr.protocol();
            assert!(protocol == Protocol::ID || protocol == Protocol::Delegated);
            assert_eq!(addr, fil_addr);
        }
    }

    #[test]
    fn test_hash() {
        let test_cases = [
            r#""0x013dbb9442ca9667baccc6230fcd5c1c4b2d4d2870f4bd20681d4d47cfd15184""#,
            r#""0xab8653edf9f51785664a643b47605a7ba3d917b5339a0724e7642c114d0e4738""#,
        ];

        for hash in test_cases {
            let h: EthHash = serde_json::from_str(hash).unwrap();

            let c = h.to_cid();
            let h1: EthHash = c.into();
            assert_eq!(h, h1);
        }
    }

    #[quickcheck]
    fn test_eth_hash_roundtrip(eth_hash: EthHash) {
        let cid = eth_hash.to_cid();
        let hash = cid.into();
        assert_eq!(eth_hash, hash);
    }

    #[test]
    fn test_block_constructor() {
        let block = Block::new(false, 1);
        assert_eq!(block.transactions_root, EthHash::empty_root());

        let block = Block::new(true, 1);
        assert_eq!(block.transactions_root, EthHash::default());
    }

    #[test]
    fn test_eth_tx_hash_from_signed_message() {
        let (_, signed) = construct_eth_messages(0);
        let tx_hash =
            eth_tx_hash_from_signed_message(&signed, crate::networks::calibnet::ETH_CHAIN_ID)
                .unwrap();
        assert_eq!(
            &format!("{tx_hash}"),
            "0xfc81dd8d9ffb045e7e2d494f925824098183263c7f402d69e18cc25e3422791b"
        );

        let (_, signed) = construct_messages();
        let tx_hash =
            eth_tx_hash_from_signed_message(&signed, crate::networks::calibnet::ETH_CHAIN_ID)
                .unwrap();
        assert_eq!(tx_hash.to_cid(), signed.cid());

        let (_, signed) = construct_bls_messages();
        let tx_hash =
            eth_tx_hash_from_signed_message(&signed, crate::networks::calibnet::ETH_CHAIN_ID)
                .unwrap();
        assert_eq!(tx_hash.to_cid(), signed.message().cid());
    }

    #[test]
    fn test_eth_tx_hash_from_message_cid() {
        let blockstore = Arc::new(MemoryDB::default());

        let (msg0, secp0) = construct_eth_messages(0);
        let (_msg1, secp1) = construct_eth_messages(1);
        let (msg2, bls0) = construct_bls_messages();

        crate::chain::persist_objects(&blockstore, [msg0.clone(), msg2.clone()].iter()).unwrap();
        crate::chain::persist_objects(&blockstore, [secp0.clone(), bls0.clone()].iter()).unwrap();

        let tx_hash = eth_tx_hash_from_message_cid(&blockstore, &secp0.cid(), 0).unwrap();
        assert!(tx_hash.is_some());

        let tx_hash = eth_tx_hash_from_message_cid(&blockstore, &msg2.cid(), 0).unwrap();
        assert!(tx_hash.is_some());

        let tx_hash = eth_tx_hash_from_message_cid(&blockstore, &secp1.cid(), 0).unwrap();
        assert!(tx_hash.is_none());
    }

    #[test]
    fn test_eth_log_from_event() {
        // The value member of these event entries correspond to existing topics on Calibnet,
        // but they could just as easily be vectors filled with random bytes.

        let entries = vec![
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t1".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t2".into(),
                codec: IPLD_RAW,
                value: vec![
                    116, 4, 227, 209, 4, 234, 120, 65, 195, 217, 230, 253, 32, 173, 254, 153, 180,
                    173, 88, 107, 192, 141, 143, 59, 211, 175, 239, 137, 76, 241, 132, 222,
                ]
                .into(),
            },
        ];
        let (bytes, hashes) = eth_log_from_event(&entries).unwrap();
        assert!(bytes.0.is_empty());
        assert_eq!(hashes.len(), 2);

        let entries = vec![
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t1".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t2".into(),
                codec: IPLD_RAW,
                value: vec![
                    116, 4, 227, 209, 4, 234, 120, 65, 195, 217, 230, 253, 32, 173, 254, 153, 180,
                    173, 88, 107, 192, 141, 143, 59, 211, 175, 239, 137, 76, 241, 132, 222,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t3".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t4".into(),
                codec: IPLD_RAW,
                value: vec![
                    116, 4, 227, 209, 4, 234, 120, 65, 195, 217, 230, 253, 32, 173, 254, 153, 180,
                    173, 88, 107, 192, 141, 143, 59, 211, 175, 239, 137, 76, 241, 132, 222,
                ]
                .into(),
            },
        ];
        let (bytes, hashes) = eth_log_from_event(&entries).unwrap();
        assert!(bytes.0.is_empty());
        assert_eq!(hashes.len(), 4);

        let entries = vec![
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t1".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t1".into(),
                codec: IPLD_RAW,
                value: vec![
                    116, 4, 227, 209, 4, 234, 120, 65, 195, 217, 230, 253, 32, 173, 254, 153, 180,
                    173, 88, 107, 192, 141, 143, 59, 211, 175, 239, 137, 76, 241, 132, 222,
                ]
                .into(),
            },
        ];
        assert!(eth_log_from_event(&entries).is_none());

        let entries = vec![
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t3".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t4".into(),
                codec: IPLD_RAW,
                value: vec![
                    116, 4, 227, 209, 4, 234, 120, 65, 195, 217, 230, 253, 32, 173, 254, 153, 180,
                    173, 88, 107, 192, 141, 143, 59, 211, 175, 239, 137, 76, 241, 132, 222,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t1".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t2".into(),
                codec: IPLD_RAW,
                value: vec![
                    116, 4, 227, 209, 4, 234, 120, 65, 195, 217, 230, 253, 32, 173, 254, 153, 180,
                    173, 88, 107, 192, 141, 143, 59, 211, 175, 239, 137, 76, 241, 132, 222,
                ]
                .into(),
            },
        ];
        let (bytes, hashes) = eth_log_from_event(&entries).unwrap();
        assert!(bytes.0.is_empty());
        assert_eq!(hashes.len(), 4);

        let entries = vec![
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t1".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t3".into(),
                codec: IPLD_RAW,
                value: vec![
                    116, 4, 227, 209, 4, 234, 120, 65, 195, 217, 230, 253, 32, 173, 254, 153, 180,
                    173, 88, 107, 192, 141, 143, 59, 211, 175, 239, 137, 76, 241, 132, 222,
                ]
                .into(),
            },
        ];
        assert!(eth_log_from_event(&entries).is_none());

        let entries = vec![EventEntry {
            flags: (Flags::FLAG_INDEXED_ALL).bits(),
            key: "t1".into(),
            codec: DAG_CBOR,
            value: vec![
                226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11, 81,
                29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
            ]
            .into(),
        }];
        assert!(eth_log_from_event(&entries).is_none());

        let entries = vec![EventEntry {
            flags: (Flags::FLAG_INDEXED_ALL).bits(),
            key: "t1".into(),
            codec: IPLD_RAW,
            value: vec![
                226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11, 81,
                29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149, 0,
            ]
            .into(),
        }];
        assert!(eth_log_from_event(&entries).is_none());

        let entries = vec![
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t1".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "d".into(),
                codec: IPLD_RAW,
                value: vec![
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 49, 190,
                    25, 34, 116, 232, 27, 26, 248,
                ]
                .into(),
            },
        ];
        let (bytes, hashes) = eth_log_from_event(&entries).unwrap();
        assert_eq!(bytes.0.len(), 32);
        assert_eq!(hashes.len(), 1);

        let entries = vec![
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t1".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149, 0,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "d".into(),
                codec: IPLD_RAW,
                value: vec![
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 49, 190,
                    25, 34, 116, 232, 27, 26, 248,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "d".into(),
                codec: IPLD_RAW,
                value: vec![
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 49, 190,
                    25, 34, 116, 232, 27, 26, 248,
                ]
                .into(),
            },
        ];
        assert!(eth_log_from_event(&entries).is_none());
    }

    #[test]
    fn test_from_bytes_valid() {
        let zero_bytes = [0u8; 32];
        assert_eq!(
            EthUint64::from_bytes(&zero_bytes).unwrap().0,
            0,
            "zero bytes"
        );

        let mut value_bytes = [0u8; 32];
        value_bytes[31] = 42;
        assert_eq!(
            EthUint64::from_bytes(&value_bytes).unwrap().0,
            42,
            "simple value"
        );

        let mut max_bytes = [0u8; 32];
        max_bytes[24..32].copy_from_slice(&u64::MAX.to_be_bytes());
        assert_eq!(
            EthUint64::from_bytes(&max_bytes).unwrap().0,
            u64::MAX,
            "valid max value"
        );
    }

    #[test]
    fn test_from_bytes_wrong_length() {
        let short_bytes = [0u8; 31];
        assert!(
            EthUint64::from_bytes(&short_bytes).is_err(),
            "bytes too short"
        );

        let long_bytes = [0u8; 33];
        assert!(
            EthUint64::from_bytes(&long_bytes).is_err(),
            "bytes too long"
        );

        let empty_bytes = [];
        assert!(
            EthUint64::from_bytes(&empty_bytes).is_err(),
            "bytes too short"
        );
    }

    #[test]
    fn test_from_bytes_overflow() {
        let mut overflow_bytes = [0u8; 32];
        overflow_bytes[10] = 1;
        assert!(
            EthUint64::from_bytes(&overflow_bytes).is_err(),
            "overflow with non-zero byte at position 10"
        );

        overflow_bytes = [0u8; 32];
        overflow_bytes[23] = 1;
        assert!(
            EthUint64::from_bytes(&overflow_bytes).is_err(),
            "overflow with non-zero byte at position 23"
        );

        overflow_bytes = [0u8; 32];
        overflow_bytes
            .iter_mut()
            .take(24)
            .for_each(|byte| *byte = 0xFF);

        assert!(
            EthUint64::from_bytes(&overflow_bytes).is_err(),
            "overflow bytes with non-zero bytes at positions 0-23"
        );

        overflow_bytes = [0u8; 32];
        for i in 0..24 {
            overflow_bytes[i] = 0xFF;
            assert!(
                EthUint64::from_bytes(&overflow_bytes).is_err(),
                "overflow with non-zero byte at position {i}"
            );
        }

        overflow_bytes = [0xFF; 32];
        assert!(
            EthUint64::from_bytes(&overflow_bytes).is_err(),
            "overflow with all ones"
        );
    }

    fn create_execution_trace(from: FilecoinAddress, to: FilecoinAddress) -> ExecutionTrace {
        ExecutionTrace {
            msg: MessageTrace {
                from,
                to,
                value: TokenAmount::default(),
                method: 0,
                params: Default::default(),
                params_codec: 0,
                gas_limit: None,
                read_only: None,
            },
            msg_rct: ReturnTrace {
                exit_code: ExitCode::from(0u32),
                r#return: Default::default(),
                return_codec: 0,
            },
            invoked_actor: None,
            gas_charges: vec![],
            subcalls: vec![],
        }
    }

    fn create_execution_trace_with_subcalls(
        from: FilecoinAddress,
        to: FilecoinAddress,
        subcalls: Vec<ExecutionTrace>,
    ) -> ExecutionTrace {
        let mut trace = create_execution_trace(from, to);
        trace.subcalls = subcalls;
        trace
    }

    #[test]
    fn test_extract_touched_addresses_with_id_addresses() {
        // ID addresses (e.g., f0100) can be converted to EthAddress
        let from = FilecoinAddress::new_id(100);
        let to = FilecoinAddress::new_id(200);
        let trace = create_execution_trace(from, to);

        let addresses = extract_touched_eth_addresses(&trace);

        assert_eq!(addresses.len(), 2);
        assert!(addresses.contains(&EthAddress::from_filecoin_address(&from).unwrap()));
        assert!(addresses.contains(&EthAddress::from_filecoin_address(&to).unwrap()));
    }

    #[test]
    fn test_extract_touched_addresses_same_from_and_to() {
        let addr = FilecoinAddress::new_id(100);
        let trace = create_execution_trace(addr, addr);

        let addresses = extract_touched_eth_addresses(&trace);

        // Should deduplicate
        assert_eq!(addresses.len(), 1);
        assert!(addresses.contains(&EthAddress::from_filecoin_address(&addr).unwrap()));
    }

    #[test]
    fn test_extract_touched_addresses_with_subcalls() {
        let addr1 = FilecoinAddress::new_id(100);
        let addr2 = FilecoinAddress::new_id(200);
        let addr3 = FilecoinAddress::new_id(300);
        let addr4 = FilecoinAddress::new_id(400);

        let subcall = create_execution_trace(addr3, addr4);
        let trace = create_execution_trace_with_subcalls(addr1, addr2, vec![subcall]);

        let addresses = extract_touched_eth_addresses(&trace);

        assert_eq!(addresses.len(), 4);
        assert!(addresses.contains(&EthAddress::from_filecoin_address(&addr1).unwrap()));
        assert!(addresses.contains(&EthAddress::from_filecoin_address(&addr2).unwrap()));
        assert!(addresses.contains(&EthAddress::from_filecoin_address(&addr3).unwrap()));
        assert!(addresses.contains(&EthAddress::from_filecoin_address(&addr4).unwrap()));
    }

    #[test]
    fn test_extract_touched_addresses_with_nested_subcalls() {
        let addr1 = FilecoinAddress::new_id(100);
        let addr2 = FilecoinAddress::new_id(200);
        let addr3 = FilecoinAddress::new_id(300);
        let addr4 = FilecoinAddress::new_id(400);
        let addr5 = FilecoinAddress::new_id(500);
        let addr6 = FilecoinAddress::new_id(600);

        // Create nested structure: trace -> subcall1 -> nested_subcall
        let nested_subcall = create_execution_trace(addr5, addr6);
        let subcall = create_execution_trace_with_subcalls(addr3, addr4, vec![nested_subcall]);
        let trace = create_execution_trace_with_subcalls(addr1, addr2, vec![subcall]);

        let addresses = extract_touched_eth_addresses(&trace);

        assert_eq!(addresses.len(), 6);
        for addr in [addr1, addr2, addr3, addr4, addr5, addr6] {
            assert!(addresses.contains(&EthAddress::from_filecoin_address(&addr).unwrap()));
        }
    }

    #[test]
    fn test_extract_touched_addresses_with_multiple_subcalls() {
        let addr1 = FilecoinAddress::new_id(100);
        let addr2 = FilecoinAddress::new_id(200);
        let addr3 = FilecoinAddress::new_id(300);
        let addr4 = FilecoinAddress::new_id(400);
        let addr5 = FilecoinAddress::new_id(500);
        let addr6 = FilecoinAddress::new_id(600);

        let subcall1 = create_execution_trace(addr3, addr4);
        let subcall2 = create_execution_trace(addr5, addr6);
        let trace = create_execution_trace_with_subcalls(addr1, addr2, vec![subcall1, subcall2]);

        let addresses = extract_touched_eth_addresses(&trace);

        assert_eq!(addresses.len(), 6);
    }

    #[test]
    fn test_extract_touched_addresses_deduplicates_across_subcalls() {
        // Same address appears in parent and subcall
        let addr1 = FilecoinAddress::new_id(100);
        let addr2 = FilecoinAddress::new_id(200);

        let subcall = create_execution_trace(addr1, addr2); // addr1 repeated
        let trace = create_execution_trace_with_subcalls(addr1, addr2, vec![subcall]);

        let addresses = extract_touched_eth_addresses(&trace);

        // Should deduplicate
        assert_eq!(addresses.len(), 2);
    }

    #[test]
    fn test_extract_touched_addresses_with_non_convertible_addresses() {
        // BLS addresses cannot be converted to EthAddress
        let bls_addr = FilecoinAddress::new_bls(&[0u8; 48]).unwrap();
        let id_addr = FilecoinAddress::new_id(100);

        let trace = create_execution_trace(bls_addr, id_addr);
        let addresses = extract_touched_eth_addresses(&trace);

        // Only the ID address should be in the set
        assert_eq!(addresses.len(), 1);
        assert!(addresses.contains(&EthAddress::from_filecoin_address(&id_addr).unwrap()));
    }
}

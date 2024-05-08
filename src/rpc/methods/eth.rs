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
use crate::rpc::error::ServerError;
use crate::rpc::{ApiVersion, Ctx, Permission, RpcMethod};
use crate::shim::address::{Address as FilecoinAddress, Protocol};
use crate::shim::crypto::{Signature, SignatureType};
use crate::shim::econ::{TokenAmount, BLOCK_GAS_LIMIT};
use crate::shim::executor::Receipt;
use crate::shim::fvm_shared_latest::address::{Address as VmAddress, DelegatedAddress};
use crate::shim::fvm_shared_latest::MethodNum;
use crate::shim::message::Message;
use crate::shim::{clock::ChainEpoch, state_tree::StateTree};

use anyhow::{bail, Result};
use bytes::{Buf, BytesMut};
use cbor4ii::core::{dec::Decode, utils::SliceReader, Value};
use cid::{
    multihash::{self, MultihashDigest},
    Cid,
};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{CBOR, DAG_CBOR, IPLD_RAW};
use itertools::Itertools;
use keccak_hash::keccak;
use num_bigint::{self, Sign};
use num_traits::{Signed as _, Zero as _};
use nunny::vec as nonempty;
use rlp::RlpStream;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use std::{ops::Add, sync::Arc};

macro_rules! for_each_method {
    ($callback:ident) => {
        $callback!(crate::rpc::eth::Web3ClientVersion);
        $callback!(crate::rpc::eth::EthSyncing);
        $callback!(crate::rpc::eth::EthAccounts);
        $callback!(crate::rpc::eth::EthBlockNumber);
        $callback!(crate::rpc::eth::EthChainId);
        $callback!(crate::rpc::eth::EthGasPrice);
        $callback!(crate::rpc::eth::EthGetBalance);
        $callback!(crate::rpc::eth::EthGetBlockByNumber);
    };
}
pub(crate) use for_each_method;

const MASKED_ID_PREFIX: [u8; 12] = [0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// Ethereum Bloom filter size in bits.
/// Bloom filter is used in Ethereum to minimize the number of block queries.
const BLOOM_SIZE: usize = 2048;

/// Ethereum Bloom filter size in bytes.
const BLOOM_SIZE_IN_BYTES: usize = BLOOM_SIZE / 8;

/// Ethereum Bloom filter with all bits set to 1.
const FULL_BLOOM: [u8; BLOOM_SIZE_IN_BYTES] = [0xff; BLOOM_SIZE_IN_BYTES];

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

/// Ethereum Improvement Proposals 1559 transaction type. This EIP changed Ethereum’s fee market mechanism.
/// Transaction type can have 3 distinct values:
/// - 0 for legacy transactions
/// - 1 for transactions introduced in EIP-2930
/// - 2 for transactions introduced in EIP-1559
const EIP_1559_TX_TYPE: u64 = 2;

/// The address used in messages to actors that have since been deleted.
const REVERTED_ETH_ADDRESS: &str = "0xff0000000000000000000000ffffffffffffffff";

#[repr(u64)]
enum EAMMethod {
    CreateExternal = 4,
}

#[repr(u64)]
enum EVMMethod {
    // it is very unfortunate but the hasher creates a circular dependency, so we use the raw
    // number.
    // InvokeContract = frc42_dispatch::method_hash!("InvokeEVM"),
    InvokeContract = 3844450837,
}

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone, JsonSchema)]
pub struct BigInt(
    #[schemars(with = "LotusJson<num_bigint::BigInt>")]
    #[serde(with = "crate::lotus_json::hexify")]
    pub num_bigint::BigInt,
);
lotus_json_with_self!(BigInt);

impl From<TokenAmount> for BigInt {
    fn from(amount: TokenAmount) -> Self {
        Self(amount.atto().to_owned())
    }
}

type GasPriceResult = BigInt;

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone, JsonSchema)]
pub struct Nonce(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::hexify_bytes")]
    pub ethereum_types::H64,
);

lotus_json_with_self!(Nonce);

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone, JsonSchema)]
pub struct Bloom(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::hexify_bytes")]
    pub ethereum_types::Bloom,
);

lotus_json_with_self!(Bloom);

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone, JsonSchema)]
pub struct Uint64(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::hexify")]
    pub u64,
);

lotus_json_with_self!(Uint64);

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone, JsonSchema)]
pub struct Bytes(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::hexify_vec_bytes")]
    pub Vec<u8>,
);

lotus_json_with_self!(Bytes);

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone, JsonSchema)]
pub struct Address(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::hexify_bytes")]
    pub ethereum_types::Address,
);

lotus_json_with_self!(Address);

impl Address {
    pub fn to_filecoin_address(&self) -> Result<FilecoinAddress, anyhow::Error> {
        if self.is_masked_id() {
            const PREFIX_LEN: usize = MASKED_ID_PREFIX.len();
            // This is a masked ID address.
            let arr = self.0.as_fixed_bytes();
            let mut bytes = [0; 8];
            bytes.copy_from_slice(&arr[PREFIX_LEN..]);
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

    // See https://github.com/filecoin-project/lotus/blob/v1.26.2/chain/types/ethtypes/eth_types.go#L347-L375 for reference implementation
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
        let pfx = MASKED_ID_PREFIX;
        let arr = id.to_be_bytes();
        let payload = [
            pfx[0], pfx[1], pfx[2], pfx[3], pfx[4], pfx[5], pfx[6], pfx[7], //
            pfx[8], pfx[9], pfx[10], pfx[11], //
            arr[0], arr[1], arr[2], arr[3], arr[4], arr[5], arr[6], arr[7],
        ];

        Self(ethereum_types::H160(payload))
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

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone, JsonSchema)]
pub struct Hash(#[schemars(with = "String")] pub ethereum_types::H256);

impl Hash {
    // Should ONLY be used for blocks and Filecoin messages. Eth transactions expect a different hashing scheme.
    pub fn to_cid(&self) -> cid::Cid {
        let mh = multihash::Code::Blake2b256.digest(self.0.as_bytes());
        Cid::new_v1(fvm_ipld_encoding::DAG_CBOR, mh)
    }

    pub fn empty_uncles() -> Self {
        Self(ethereum_types::H256::from_str(EMPTY_UNCLES).unwrap())
    }

    pub fn empty_root() -> Self {
        Self(ethereum_types::H256::from_str(EMPTY_ROOT).unwrap())
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

// TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/4032
//                  this shouldn't exist
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

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(PartialEq, Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
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
    pub fn new(has_transactions: bool) -> Self {
        Self {
            gas_limit: Uint64(BLOCK_GAS_LIMIT),
            logs_bloom: Bloom(ethereum_types::Bloom(FULL_BLOOM)),
            sha3_uncles: Hash::empty_uncles(),
            transactions_root: if has_transactions {
                Hash::default()
            } else {
                Hash::empty_root()
            },
            ..Default::default()
        }
    }
}

lotus_json_with_self!(Block);

#[derive(PartialEq, Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Tx {
    pub chain_id: Uint64,
    pub nonce: Uint64,
    pub hash: Hash,
    pub block_hash: Hash,
    pub block_number: Uint64,
    pub transaction_index: Uint64,
    pub from: Address,
    #[schemars(with = "Option<Address>")]
    #[serde(
        with = "crate::lotus_json",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub to: Option<Address>,
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
    pub fn eth_hash(&self) -> Result<Hash> {
        let eth_tx_args: TxArgs = self.clone().into();
        eth_tx_args.hash()
    }
}

lotus_json_with_self!(Tx);

#[derive(PartialEq, Debug, Clone, Default)]
struct TxArgs {
    pub chain_id: u64,
    pub nonce: u64,
    pub to: Option<Address>,
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

fn format_u64(value: u64) -> BytesMut {
    if value != 0 {
        let i = (value.leading_zeros() / 8) as usize;
        let bytes = value.to_be_bytes();
        // `leading_zeros` for a positive `u64` returns a number in the range [1-63]
        // `i` is in the range [1-7], and `bytes` is an array of size 8
        // therefore, getting the slice from `i` to end should never fail
        bytes.get(i..).expect("failed to get slice").into()
    } else {
        // If all bytes are zero, return an empty slice
        BytesMut::new()
    }
}

fn format_bigint(value: &BigInt) -> Result<BytesMut> {
    Ok(if value.0.is_positive() {
        BytesMut::from_iter(value.0.to_bytes_be().1.iter())
    } else {
        if value.0.is_negative() {
            bail!("can't format a negative number");
        }
        // If all bytes are zero, return an empty slice
        BytesMut::new()
    })
}

fn format_address(value: &Option<Address>) -> BytesMut {
    if let Some(addr) = value {
        addr.0.as_bytes().into()
    } else {
        BytesMut::new()
    }
}

impl TxArgs {
    pub fn hash(&self) -> Result<Hash> {
        Ok(Hash(keccak(self.rlp_signed_message()?)))
    }

    pub fn rlp_signed_message(&self) -> Result<Vec<u8>> {
        // An item is either an item list or bytes.
        const MSG_ITEMS: usize = 12;

        let mut stream = RlpStream::new_list(MSG_ITEMS);
        stream.append(&format_u64(self.chain_id));
        stream.append(&format_u64(self.nonce));
        stream.append(&format_bigint(&self.max_priority_fee_per_gas)?);
        stream.append(&format_bigint(&self.max_fee_per_gas)?);
        stream.append(&format_u64(self.gas_limit));
        stream.append(&format_address(&self.to));
        stream.append(&format_bigint(&self.value)?);
        stream.append(&self.input);
        let access_list: &[u8] = &[];
        stream.append_list(access_list);

        stream.append(&format_bigint(&self.v)?);
        stream.append(&format_bigint(&self.r)?);
        stream.append(&format_bigint(&self.s)?);

        let mut rlp = stream.out()[..].to_vec();
        let mut bytes: Vec<u8> = vec![0x02];
        bytes.append(&mut rlp);

        let hex = bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join("");
        tracing::trace!("rlp: {}", &hex);

        Ok(bytes)
    }
}

#[derive(Debug, Clone, Default)]
pub struct EthSyncingResult {
    pub done_sync: bool,
    pub starting_block: i64,
    pub current_block: i64,
    pub highest_block: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

// TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/4032
//                  this shouldn't exist
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

pub enum Web3ClientVersion {}
impl RpcMethod<0> for Web3ClientVersion {
    const NAME: &'static str = "Filecoin.Web3ClientVersion";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_VERSION: ApiVersion = ApiVersion::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = String;

    async fn handle(
        _: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        Ok(crate::utils::version::FOREST_VERSION_STRING.clone())
    }
}

pub enum EthAccounts {}
impl RpcMethod<0> for EthAccounts {
    const NAME: &'static str = "Filecoin.EthAccounts";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_VERSION: ApiVersion = ApiVersion::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = Vec<String>;

    async fn handle(
        _: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        // EthAccounts will always return [] since we don't expect Forest to manage private keys
        Ok(vec![])
    }
}

pub enum EthBlockNumber {}
impl RpcMethod<0> for EthBlockNumber {
    const NAME: &'static str = "Filecoin.EthBlockNumber";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_VERSION: ApiVersion = ApiVersion::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = String;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        // `eth_block_number` needs to return the height of the latest committed tipset.
        // Ethereum clients expect all transactions included in this block to have execution outputs.
        // This is the parent of the head tipset. The head tipset is speculative, has not been
        // recognized by the network, and its messages are only included, not executed.
        // See https://github.com/filecoin-project/ref-fvm/issues/1135.
        let heaviest = ctx.state_manager.chain_store().heaviest_tipset();
        if heaviest.epoch() == 0 {
            // We're at genesis.
            return Ok("0x0".to_string());
        }
        // First non-null parent.
        let effective_parent = heaviest.parents();
        if let Ok(Some(parent)) = ctx.chain_store.chain_index.load_tipset(effective_parent) {
            Ok(format!("{:#x}", parent.epoch()))
        } else {
            Ok("0x0".to_string())
        }
    }
}

pub enum EthChainId {}
impl RpcMethod<0> for EthChainId {
    const NAME: &'static str = "Filecoin.EthChainId";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_VERSION: ApiVersion = ApiVersion::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = String;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        Ok(format!(
            "{:#x}",
            ctx.state_manager.chain_config().eth_chain_id
        ))
    }
}

pub enum EthGasPrice {}
impl RpcMethod<0> for EthGasPrice {
    const NAME: &'static str = "Filecoin.EthGasPrice";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_VERSION: ApiVersion = ApiVersion::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = GasPriceResult;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.state_manager.chain_store().heaviest_tipset();
        let block0 = ts.block_headers().first();
        let base_fee = &block0.parent_base_fee;
        if let Ok(premium) = gas::estimate_gas_premium(&ctx, 10000).await {
            let gas_price = base_fee.add(premium);
            Ok(BigInt(gas_price.atto().clone()))
        } else {
            Ok(BigInt(num_bigint::BigInt::zero()))
        }
    }
}

pub enum EthGetBalance {}
impl RpcMethod<2> for EthGetBalance {
    const NAME: &'static str = "Filecoin.EthGetBalance";
    const PARAM_NAMES: [&'static str; 2] = ["address", "block_param"];
    const API_VERSION: ApiVersion = ApiVersion::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (Address, BlockNumberOrHash);
    type Ok = BigInt;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, block_param): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let fil_addr = address.to_filecoin_address()?;
        let ts = tipset_by_block_number_or_hash(&ctx.chain_store, block_param)?;
        let state =
            StateTree::new_from_root(ctx.state_manager.blockstore_owned(), ts.parent_state())?;
        let actor = state.get_required_actor(&fil_addr)?;
        Ok(BigInt(actor.balance.atto().clone()))
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
) -> Result<(Cid, Vec<(ChainMessage, Receipt)>)> {
    let msgs = data.chain_store.messages_for_tipset(tipset)?;

    let (state_root, receipt_root) = data.state_manager.tipset_state(tipset).await?;

    let receipts = Receipt::get_receipts(data.state_manager.blockstore(), receipt_root)?;

    if msgs.len() != receipts.len() {
        bail!(
            "receipts and message array lengths didn't match for tipset: {:?}",
            tipset
        )
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

fn eth_tx_args_from_unsigned_eth_message(msg: &Message) -> Result<TxArgs> {
    let mut to = None;
    let mut params = vec![];

    if msg.version != 0 {
        bail!("unsupported msg version: {}", msg.version);
    }

    if !msg.params().bytes().is_empty() {
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
        to = Some(addr);
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

    let r = num_bigint::BigInt::from_bytes_be(
        Sign::Plus,
        sig.bytes().get(0..32).expect("failed to get slice"),
    );

    let s = num_bigint::BigInt::from_bytes_be(
        Sign::Plus,
        sig.bytes().get(32..64).expect("failed to get slice"),
    );

    let v = num_bigint::BigInt::from_bytes_be(
        Sign::Plus,
        sig.bytes().get(64..65).expect("failed to get slice"),
    );

    Ok((BigInt(r), BigInt(s), BigInt(v)))
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
) -> Result<Option<Address>> {
    // Attempt to convert directly, if it's an f4 address.
    if let Ok(eth_addr) = Address::from_filecoin_address(addr) {
        if !eth_addr.is_masked_id() {
            return Ok(Some(eth_addr));
        }
    }

    // Otherwise, resolve the ID addr.
    let id_addr = match state.lookup_id(addr)? {
        Some(id) => id,
        _ => return Ok(None),
    };

    // Lookup on the target actor and try to get an f410 address.
    let result = state.get_actor(addr);
    if let Ok(Some(actor_state)) = result {
        if let Some(addr) = actor_state.delegated_address {
            if let Ok(eth_addr) = Address::from_filecoin_address(&addr.into()) {
                if !eth_addr.is_masked_id() {
                    // Conversable into an eth address, use it.
                    return Ok(Some(eth_addr));
                }
            }
        } else {
            // No delegated address -> use a masked ID address
        }
    } else if let Ok(None) = result {
        // Not found -> use a masked ID address
    } else {
        // Any other error -> fail.
        result?;
    }

    // Otherwise, use the masked address.
    Ok(Some(Address::from_actor_id(id_addr)))
}

/// See <https://docs.soliditylang.org/en/latest/abi-spec.html#function-selector-and-argument-encoding>
/// for ABI specification
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

/// Round to the next multiple of `EVM` word length.
fn round_up_word(value: usize) -> usize {
    ((value + (EVM_WORD_LENGTH - 1)) / EVM_WORD_LENGTH) * EVM_WORD_LENGTH
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
        .chain(std::iter::repeat(&0u8).take(round_up_word(data.len()) - data.len())) // Left pad
        .cloned()
        .collect();

    buf
}

/// Decodes the payload using the given codec.
fn decode_payload(payload: &fvm_ipld_encoding::RawBytes, codec: u64) -> Result<Bytes> {
    match codec {
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
        Ok(None) => Some(Address(
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
        if msg.method_num() == EVMMethod::InvokeContract as MethodNum
            || msg.method_num() == EAMMethod::CreateExternal as MethodNum
        {
            if let Ok(buffer) = decode_payload(msg.params(), codec) {
                // If this is a valid "create external", unset the "to" address.
                if msg.method_num() == EAMMethod::CreateExternal as MethodNum {
                    to = None;
                }
                break 'decode buffer;
            }
            // Yeah, we're going to ignore errors here because the user can send whatever they
            // want and may send garbage.
        }
        encode_filecoin_params_as_abi(msg.method_num(), codec, msg.params())?
    };

    Ok(Tx {
        to,
        from,
        input,
        nonce: Uint64(msg.sequence),
        chain_id: Uint64(chain_id as u64),
        value: msg.value.clone().into(),
        r#type: Uint64(EIP_1559_TX_TYPE),
        gas: Uint64(msg.gas_limit),
        max_fee_per_gas: msg.gas_fee_cap.clone().into(),
        max_priority_fee_per_gas: msg.gas_premium.clone().into(),
        access_list: vec![],
        ..Tx::default()
    })
}

pub fn new_eth_tx_from_signed_message<DB: Blockstore>(
    smsg: &SignedMessage,
    state: &StateTree<DB>,
    chain_id: u32,
) -> Result<Tx> {
    let (tx, hash) = if smsg.is_delegated() {
        // This is an eth tx
        let tx = eth_tx_from_signed_eth_message(smsg, chain_id)?;
        let hash = tx.eth_hash()?;
        (tx, hash)
    } else if smsg.is_secp256k1() {
        // Secp Filecoin Message
        let tx = eth_tx_from_native_message(smsg.message(), state, chain_id)?;
        (tx, smsg.cid()?.into())
    } else {
        // BLS Filecoin message
        let tx = eth_tx_from_native_message(smsg.message(), state, chain_id)?;
        (tx, smsg.message().cid()?.into())
    };
    Ok(Tx { hash, ..tx })
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

    let (state_root, msgs_and_receipts) = execute_tipset(data.clone(), &tipset).await?;

    let state_tree = StateTree::new_from_root(data.state_manager.blockstore_owned(), &state_root)?;

    let mut full_transactions = vec![];
    let mut hash_transactions = vec![];
    let mut gas_used = 0;
    for (i, (msg, receipt)) in msgs_and_receipts.iter().enumerate() {
        let ti = Uint64(i as u64);
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
            data.state_manager.chain_config().eth_chain_id,
        )?;
        tx.block_hash = block_hash.clone();
        tx.block_number = block_number.clone();
        tx.transaction_index = ti;

        if full_tx_info {
            full_transactions.push(tx);
        } else {
            hash_transactions.push(tx.hash.to_string());
        }
    }

    Ok(Block {
        hash: block_hash,
        number: block_number,
        parent_hash: parent_cid.into(),
        timestamp: Uint64(tipset.block_headers().first().timestamp),
        base_fee_per_gas: tipset
            .block_headers()
            .first()
            .parent_base_fee
            .clone()
            .into(),
        gas_used: Uint64(gas_used),
        transactions: if full_tx_info {
            Transactions::Full(full_transactions)
        } else {
            Transactions::Hash(hash_transactions)
        },
        ..Block::new(!msgs_and_receipts.is_empty())
    })
}

pub enum EthGetBlockByNumber {}
impl RpcMethod<2> for EthGetBlockByNumber {
    const NAME: &'static str = "Filecoin.EthGetBlockByNumber";
    const PARAM_NAMES: [&'static str; 2] = ["block_param", "full_tx_info"];
    const API_VERSION: ApiVersion = ApiVersion::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (BlockNumberOrHash, bool);
    type Ok = Block;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_param, full_tx_info): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash(&ctx.chain_store, block_param)?;
        let block = block_from_filecoin_tipset(ctx, ts, full_tx_info).await?;
        Ok(block)
    }
}

pub enum EthSyncing {}
impl RpcMethod<0> for EthSyncing {
    const NAME: &'static str = "Filecoin.EthSyncing";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_VERSION: ApiVersion = ApiVersion::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = EthSyncingResult;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let crate::rpc::sync::RPCSyncState { active_syncs } =
            crate::rpc::sync::SyncState::handle(ctx, ()).await?;
        match active_syncs
            .into_iter()
            .rev()
            .find_or_first(|ss| ss.stage() != SyncStage::Idle)
        {
            Some(sync_state) => match (sync_state.base(), sync_state.target()) {
                (Some(base), Some(target)) => Ok(EthSyncingResult {
                    done_sync: sync_state.stage() == SyncStage::Complete,
                    current_block: sync_state.epoch(),
                    starting_block: base.epoch(),
                    highest_block: target.epoch(),
                }),
                _ => Err(ServerError::internal_error(
                    "missing syncing information, try again",
                    None,
                )),
            },
            None => Err(ServerError::internal_error("sync state not found", None)),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use ethereum_types::H160;
    use num_bigint;
    use num_traits::{FromBytes, Signed};
    use quickcheck_macros::quickcheck;
    use std::num::ParseIntError;

    #[quickcheck]
    fn gas_price_result_serde_roundtrip(i: u128) {
        let r = BigInt(i.into());
        let encoded = serde_json::to_string(&r).unwrap();
        assert_eq!(encoded, format!("\"{i:#x}\""));
        let decoded: BigInt = serde_json::from_str(&encoded).unwrap();
        assert_eq!(r.0, decoded.0);
    }

    fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
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
    fn test_abi_encoding_empty_bytes() {
        // Generated using https://abi.hashex.org/
        const EXPECTED: &str = "0000000000000000000000000000000000000000000000000000000000000016000000000000000000000000000000000000000000000000000000000000005100000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000000";
        let expected_bytes = decode_hex(EXPECTED).unwrap();
        let data_bytes = vec![];

        assert_eq!(expected_bytes, encode_as_abi_helper(22, 81, &data_bytes));
    }

    #[test]
    fn test_abi_encoding_one_byte() {
        // According to https://docs.soliditylang.org/en/latest/abi-spec.html and handcrafted
        // Uint64, Uint64, Bytes[]
        // 22, 81, [253]
        const EXPECTED: &str = "0000000000000000000000000000000000000000000000000000000000000016000000000000000000000000000000000000000000000000000000000000005100000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000001fd00000000000000000000000000000000000000000000000000000000000000";
        let expected_bytes = decode_hex(EXPECTED).unwrap();
        let data_bytes = vec![253];

        assert_eq!(expected_bytes, encode_as_abi_helper(22, 81, &data_bytes));
    }

    #[test]
    fn test_rlp_encoding() {
        let eth_tx_args = TxArgs {
            chain_id: 314159,
            nonce: 486,
            to: Some(Address(
                ethereum_types::H160::from_str("0xeb4a9cdb9f42d3a503d580a39b6e3736eb21fffd")
                    .unwrap(),
            )),
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
        assert_eq!(expected_hash, eth_tx_args.hash().unwrap());
    }

    #[quickcheck]
    fn u64_roundtrip(i: u64) {
        let bm = format_u64(i);
        if i == 0 {
            assert!(bm.is_empty());
        } else {
            // check that buffer doesn't start with zero
            let freezed = bm.freeze();
            assert!(!freezed.starts_with(&[0]));

            // roundtrip
            let mut padded = [0u8; 8];
            let bytes: &[u8] = &freezed.slice(..);
            padded[8 - bytes.len()..].copy_from_slice(bytes);
            assert_eq!(i, u64::from_be_bytes(padded));
        }
    }

    #[quickcheck]
    fn bigint_roundtrip(bi: num_bigint::BigInt) {
        let eth_bi = BigInt(bi.clone());

        match format_bigint(&eth_bi) {
            Ok(bm) => {
                if eth_bi.0.is_zero() {
                    assert!(bm.is_empty());
                } else {
                    // check that buffer doesn't start with zero
                    let freezed = bm.freeze();
                    assert!(!freezed.starts_with(&[0]));

                    // roundtrip
                    let unsigned = num_bigint::BigUint::from_be_bytes(&freezed.slice(..));
                    assert_eq!(bi, unsigned.into());
                }
            }
            Err(_) => {
                // fails in case of negative number
                assert!(eth_bi.0.is_negative());
            }
        }
    }

    #[test]
    fn test_id_address_roundtrip() {
        let test_cases = [1u64, 2, 3, 100, 101];

        for id in test_cases {
            let addr = FilecoinAddress::new_id(id);

            // roundtrip
            let eth_addr = Address::from_filecoin_address(&addr).unwrap();
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
            let eth_addr: Address = serde_json::from_str(addr).unwrap();

            let encoded = serde_json::to_string(&eth_addr).unwrap();
            assert_eq!(encoded, addr.to_lowercase());

            let decoded: Address = serde_json::from_str(&encoded).unwrap();
            assert_eq!(eth_addr, decoded);
        }
    }

    #[quickcheck]
    fn test_fil_address_roundtrip(addr: FilecoinAddress) {
        if let Ok(eth_addr) = Address::from_filecoin_address(&addr) {
            let fil_addr = eth_addr.to_filecoin_address().unwrap();

            let protocol = addr.protocol();
            assert!(protocol == Protocol::ID || protocol == Protocol::Delegated);
            assert_eq!(addr, fil_addr);
        }
    }

    #[test]
    fn test_block_constructor() {
        let block = Block::new(false);
        assert_eq!(block.transactions_root, Hash::empty_root());

        let block = Block::new(true);
        assert_eq!(block.transactions_root, Hash::default());
    }

    #[test]
    fn test_tx_args_to_address_is_none() {
        let msg = Message {
            version: 0,
            to: FilecoinAddress::ETHEREUM_ACCOUNT_MANAGER_ACTOR,
            method_num: EAMMethod::CreateExternal as u64,
            ..Message::default()
        };

        assert!(eth_tx_args_from_unsigned_eth_message(&msg)
            .unwrap()
            .to
            .is_none());
    }

    #[test]
    fn test_tx_args_to_address_is_some() {
        let msg = Message {
            version: 0,
            to: FilecoinAddress::from_str("f410fujiqghwwwr3z4kqlse3ihzyqipmiaavdqchxs2y").unwrap(),
            method_num: EVMMethod::InvokeContract as u64,
            ..Message::default()
        };

        assert_eq!(
            eth_tx_args_from_unsigned_eth_message(&msg)
                .unwrap()
                .to
                .unwrap(),
            Address(H160::from_str("0xa251031ed6b4779e2a0b913683e71043d88002a3").unwrap())
        );
    }
}

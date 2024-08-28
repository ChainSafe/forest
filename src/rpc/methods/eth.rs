// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod eth_tx;
pub mod filter;
pub mod types;

use self::eth_tx::*;
use self::types::*;
use super::gas;
use crate::blocks::Tipset;
use crate::chain::{index::ResolveNullTipset, ChainStore};
use crate::chain_sync::SyncStage;
use crate::cid_collections::CidHashSet;
use crate::eth::SAFE_EPOCH_DELAY;
use crate::eth::{
    EAMMethod, EVMMethod, EthChainId as EthChainIdType, EthEip1559TxArgs, EthLegacyEip155TxArgs,
    EthLegacyHomesteadTxArgs,
};
use crate::interpreter::VMTrace;
use crate::lotus_json::{lotus_json_with_self, HasLotusJson};
use crate::message::{ChainMessage, Message as _, SignedMessage};
use crate::rpc::error::ServerError;
use crate::rpc::types::{ApiTipsetKey, MessageLookup};
use crate::rpc::{ApiPaths, Ctx, Permission, RpcMethod};
use crate::shim::actors::is_evm_actor;
use crate::shim::actors::EVMActorStateLoad as _;
use crate::shim::address::{Address as FilecoinAddress, Protocol};
use crate::shim::crypto::Signature;
use crate::shim::econ::{TokenAmount, BLOCK_GAS_LIMIT};
use crate::shim::error::ExitCode;
use crate::shim::executor::Receipt;
use crate::shim::fvm_shared_latest::address::{Address as VmAddress, DelegatedAddress};
use crate::shim::fvm_shared_latest::MethodNum;
use crate::shim::message::Message;
use crate::shim::trace::{CallReturn, ExecutionEvent};
use crate::shim::{clock::ChainEpoch, state_tree::StateTree};
use crate::utils::db::BlockstoreExt as _;
use anyhow::{bail, Context, Result};
use cbor4ii::core::dec::Decode as _;
use cbor4ii::core::Value;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{RawBytes, CBOR, DAG_CBOR, IPLD_RAW};
use itertools::Itertools;
use libipld_core::ipld::Ipld;
use num::{BigInt, Zero as _};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::{ops::Add, sync::Arc};

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

/// The address used in messages to actors that have since been deleted.
const REVERTED_ETH_ADDRESS: &str = "0xff0000000000000000000000ffffffffffffffff";

// TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/4436
//                  use ethereum_types::U256 or use lotus_json::big_int
#[derive(
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

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone, JsonSchema)]
pub struct Bloom(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::hexify_bytes")]
    pub ethereum_types::Bloom,
);

lotus_json_with_self!(Bloom);

#[derive(
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
pub struct Uint64(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::hexify")]
    pub u64,
);

lotus_json_with_self!(Uint64);

#[derive(
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
pub struct Int64(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::hexify")]
    pub i64,
);

lotus_json_with_self!(Int64);

impl EthHash {
    // Should ONLY be used for blocks and Filecoin messages. Eth transactions expect a different hashing scheme.
    pub fn to_cid(&self) -> cid::Cid {
        use cid::multihash::MultihashDigest;

        let mh = cid::multihash::Code::Blake2b256
            .wrap(self.0.as_bytes())
            .expect("should not fail");
        Cid::new_v1(fvm_ipld_encoding::DAG_CBOR, mh)
    }

    pub fn empty_uncles() -> Self {
        Self(ethereum_types::H256::from_str(EMPTY_UNCLES).unwrap())
    }

    pub fn empty_root() -> Self {
        Self(ethereum_types::H256::from_str(EMPTY_ROOT).unwrap())
    }
}

impl FromStr for EthHash {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(EthHash(ethereum_types::H256::from_str(s)?))
    }
}

impl From<Cid> for EthHash {
    fn from(cid: Cid) -> Self {
        let (_, digest, _) = cid.hash().into_inner();
        EthHash(ethereum_types::H256::from_slice(&digest[0..32]))
    }
}

lotus_json_with_self!(EthHash);

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Predefined {
    Earliest,
    Pending,
    #[default]
    Latest,
    Safe,
    Finalized,
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlockNumber {
    block_number: Int64,
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlockHash {
    block_hash: EthHash,
    require_canonical: bool,
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BlockNumberOrHash {
    #[schemars(with = "String")]
    PredefinedBlock(Predefined),
    BlockNumber(Int64),
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
        Self::BlockNumber(Int64(number))
    }

    pub fn from_block_hash(hash: EthHash) -> Self {
        Self::BlockHash(hash)
    }

    /// Construct a block number using EIP-1898 Object scheme.
    ///
    /// For details see <https://eips.ethereum.org/EIPS/eip-1898>
    pub fn from_block_number_object(number: i64) -> Self {
        Self::BlockNumberObject(BlockNumber {
            block_number: Int64(number),
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
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)] // try a Vec<String>, then a Vec<Tx>
pub enum Transactions {
    Hash(Vec<String>),
    Full(Vec<ApiEthTx>),
}

impl Default for Transactions {
    fn default() -> Self {
        Self::Hash(vec![])
    }
}

#[derive(PartialEq, Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
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
    pub difficulty: Uint64,
    pub total_difficulty: Uint64,
    pub number: Uint64,
    pub gas_limit: Uint64,
    pub gas_used: Uint64,
    pub timestamp: Uint64,
    pub extra_data: EthBytes,
    pub mix_hash: EthHash,
    pub nonce: Nonce,
    pub base_fee_per_gas: EthBigInt,
    pub size: Uint64,
    // can be Vec<Tx> or Vec<String> depending on query params
    pub transactions: Transactions,
    pub uncles: Vec<EthHash>,
}

impl Block {
    pub fn new(has_transactions: bool, tipset_len: usize) -> Self {
        Self {
            gas_limit: Uint64(BLOCK_GAS_LIMIT.saturating_mul(tipset_len as _)),
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
}

lotus_json_with_self!(Block);

#[derive(PartialEq, Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiEthTx {
    pub chain_id: Uint64,
    pub nonce: Uint64,
    pub hash: EthHash,
    pub block_hash: EthHash,
    pub block_number: Uint64,
    pub transaction_index: Uint64,
    pub from: EthAddress,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub to: Option<EthAddress>,
    pub value: EthBigInt,
    pub r#type: Uint64,
    pub input: EthBytes,
    pub gas: Uint64,
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
    const NAME_ALIAS: Option<&'static str> = Some("web3_clientVersion");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: ApiPaths = ApiPaths::V1;
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
    const NAME_ALIAS: Option<&'static str> = Some("eth_accounts");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: ApiPaths = ApiPaths::V1;
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
    const NAME_ALIAS: Option<&'static str> = Some("eth_blockNumber");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: ApiPaths = ApiPaths::V1;
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
        let heaviest = ctx.chain_store().heaviest_tipset();
        if heaviest.epoch() == 0 {
            // We're at genesis.
            return Ok("0x0".to_string());
        }
        // First non-null parent.
        let effective_parent = heaviest.parents();
        if let Ok(Some(parent)) = ctx.chain_index().load_tipset(effective_parent) {
            Ok(format!("{:#x}", parent.epoch()))
        } else {
            Ok("0x0".to_string())
        }
    }
}

pub enum EthChainId {}
impl RpcMethod<0> for EthChainId {
    const NAME: &'static str = "Filecoin.EthChainId";
    const NAME_ALIAS: Option<&'static str> = Some("eth_chainId");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = String;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        Ok(format!("{:#x}", ctx.chain_config().eth_chain_id))
    }
}

pub enum EthGasPrice {}
impl RpcMethod<0> for EthGasPrice {
    const NAME: &'static str = "Filecoin.EthGasPrice";
    const NAME_ALIAS: Option<&'static str> = Some("eth_gasPrice");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = GasPriceResult;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx.chain_store().heaviest_tipset();
        let block0 = ts.block_headers().first();
        let base_fee = &block0.parent_base_fee;
        if let Ok(premium) = gas::estimate_gas_premium(&ctx, 10000).await {
            let gas_price = base_fee.add(premium);
            Ok(EthBigInt(gas_price.atto().clone()))
        } else {
            Ok(EthBigInt(BigInt::zero()))
        }
    }
}

pub enum EthGetBalance {}
impl RpcMethod<2> for EthGetBalance {
    const NAME: &'static str = "Filecoin.EthGetBalance";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getBalance");
    const PARAM_NAMES: [&'static str; 2] = ["address", "block_param"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthAddress, BlockNumberOrHash);
    type Ok = EthBigInt;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (address, block_param): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let fil_addr = address.to_filecoin_address()?;
        let ts = tipset_by_block_number_or_hash(ctx.chain_store(), block_param)?;
        let state = StateTree::new_from_root(ctx.store_owned(), ts.parent_state())?;
        let actor = state.get_required_actor(&fil_addr)?;
        Ok(EthBigInt(actor.balance.atto().clone()))
    }
}

fn get_tipset_from_hash<DB: Blockstore>(
    chain_store: &ChainStore<DB>,
    block_hash: &EthHash,
) -> anyhow::Result<Tipset> {
    let tsk = chain_store.get_required_tipset_key(block_hash)?;
    Tipset::load_required(chain_store.blockstore(), &tsk)
}

fn tipset_by_block_number_or_hash<DB: Blockstore>(
    chain: &ChainStore<DB>,
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
            Predefined::Safe => {
                let latest_height = head.epoch() - 1;
                let safe_height = latest_height - SAFE_EPOCH_DELAY;
                let ts = chain.chain_index.tipset_by_height(
                    safe_height,
                    head,
                    ResolveNullTipset::TakeOlder,
                )?;
                Ok(ts)
            }
            Predefined::Finalized => {
                let latest_height = head.epoch() - 1;
                let finality_height = latest_height - chain.chain_config.policy.chain_finality;
                let ts = chain.chain_index.tipset_by_height(
                    finality_height,
                    head,
                    ResolveNullTipset::TakeOlder,
                )?;
                Ok(ts)
            }
        },
        BlockNumberOrHash::BlockNumber(block_number)
        | BlockNumberOrHash::BlockNumberObject(BlockNumber { block_number }) => {
            let height = ChainEpoch::from(block_number.0);
            if height > head.epoch() - 1 {
                bail!("requested a future epoch (beyond \"latest\")");
            }
            let ts =
                chain
                    .chain_index
                    .tipset_by_height(height, head, ResolveNullTipset::TakeOlder)?;
            Ok(ts)
        }
        BlockNumberOrHash::BlockHash(block_hash) => {
            let ts = Arc::new(get_tipset_from_hash(chain, &block_hash)?);
            Ok(ts)
        }
        BlockNumberOrHash::BlockHashObject(BlockHash {
            block_hash,
            require_canonical,
        }) => {
            let ts = Arc::new(get_tipset_from_hash(chain, &block_hash)?);
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
    data: &Ctx<DB>,
    tipset: &Arc<Tipset>,
) -> Result<(Cid, Vec<(ChainMessage, Receipt)>)> {
    let msgs = data.chain_store().messages_for_tipset(tipset)?;

    let (state_root, receipt_root) = data.state_manager.tipset_state(tipset).await?;

    let receipts = Receipt::get_receipts(data.store(), receipt_root)?;

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

fn lookup_eth_address<DB: Blockstore>(
    addr: &FilecoinAddress,
    state: &StateTree<DB>,
) -> Result<Option<EthAddress>> {
    // Attempt to convert directly, if it's an f4 address.
    if let Ok(eth_addr) = EthAddress::from_filecoin_address(addr) {
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
            if let Ok(eth_addr) = EthAddress::from_filecoin_address(&addr.into()) {
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
    Ok(Some(EthAddress::from_actor_id(id_addr)))
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
fn decode_payload(payload: &fvm_ipld_encoding::RawBytes, codec: u64) -> Result<EthBytes> {
    match codec {
        DAG_CBOR | CBOR => {
            let mut reader = cbor4ii::core::utils::SliceReader::new(payload.bytes());
            match Value::decode(&mut reader) {
                Ok(Value::Bytes(bytes)) => Ok(EthBytes(bytes)),
                _ => bail!("failed to read params byte array"),
            }
        }
        IPLD_RAW => Ok(EthBytes(payload.to_vec())),
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

    Ok(ApiEthTx {
        to,
        from,
        input,
        nonce: Uint64(msg.sequence),
        chain_id: Uint64(chain_id),
        value: msg.value.clone().into(),
        r#type: Uint64(EIP_1559_TX_TYPE),
        gas: Uint64(msg.gas_limit),
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

    let state = StateTree::new_from_root(ctx.store().into(), ts.parent_state())?;

    Ok(ApiEthTx {
        block_hash: parent_ts_cid.into(),
        block_number: (parent_ts.epoch() as u64).into(),
        transaction_index: tx_index.into(),
        ..new_eth_tx_from_signed_message(&smsg, &state, ctx.chain_config().eth_chain_id)?
    })
}

fn get_signed_message<DB: Blockstore>(ctx: &Ctx<DB>, message_cid: Cid) -> Result<SignedMessage> {
    let result: Result<SignedMessage, crate::chain::Error> =
        crate::chain::message_from_cid(ctx.store(), &message_cid);

    result.or_else(|_| {
        // We couldn't find the signed message, it might be a BLS message, so search for a regular message.
        let msg: Message = crate::chain::message_from_cid(ctx.store(), &message_cid)
            .with_context(|| format!("failed to find msg {}", message_cid))?;
        Ok(SignedMessage::new_unchecked(
            msg,
            Signature::new_bls(vec![]),
        ))
    })
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
    let block_hash: EthHash = block_cid.into();

    let (state_root, msgs_and_receipts) = execute_tipset(&data, &tipset).await?;

    let state_tree = StateTree::new_from_root(data.store_owned(), &state_root)?;

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

        let mut tx =
            new_eth_tx_from_signed_message(&smsg, &state_tree, data.chain_config().eth_chain_id)?;
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
        ..Block::new(!msgs_and_receipts.is_empty(), tipset.len())
    })
}

pub enum EthGetBlockByHash {}
impl RpcMethod<2> for EthGetBlockByHash {
    const NAME: &'static str = "Filecoin.EthGetBlockByHash";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getBlockByHash");
    const PARAM_NAMES: [&'static str; 2] = ["block_param", "full_tx_info"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (BlockNumberOrHash, bool);
    type Ok = Block;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_param, full_tx_info): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash(ctx.chain_store(), block_param)?;
        let block = block_from_filecoin_tipset(ctx, ts, full_tx_info).await?;
        Ok(block)
    }
}

pub enum EthGetBlockByNumber {}
impl RpcMethod<2> for EthGetBlockByNumber {
    const NAME: &'static str = "Filecoin.EthGetBlockByNumber";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getBlockByNumber");
    const PARAM_NAMES: [&'static str; 2] = ["block_param", "full_tx_info"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (BlockNumberOrHash, bool);
    type Ok = Block;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_param, full_tx_info): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash(ctx.chain_store(), block_param)?;
        let block = block_from_filecoin_tipset(ctx, ts, full_tx_info).await?;
        Ok(block)
    }
}

pub enum EthGetBlockTransactionCountByHash {}
impl RpcMethod<1> for EthGetBlockTransactionCountByHash {
    const NAME: &'static str = "Filecoin.EthGetBlockTransactionCountByHash";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getBlockTransactionCountByHash");
    const PARAM_NAMES: [&'static str; 1] = ["block_hash"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthHash,);
    type Ok = Uint64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_hash,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = get_tipset_from_hash(ctx.chain_store(), &block_hash)?;

        let head = ctx.chain_store().heaviest_tipset();
        if ts.epoch() > head.epoch() {
            return Err(anyhow::anyhow!("requested a future epoch (beyond \"latest\")").into());
        }
        let count = count_messages_in_tipset(ctx.store(), &ts)?;
        Ok(Uint64(count as _))
    }
}

pub enum EthGetBlockTransactionCountByNumber {}
impl RpcMethod<1> for EthGetBlockTransactionCountByNumber {
    const NAME: &'static str = "Filecoin.EthGetBlockTransactionCountByNumber";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getBlockTransactionCountByNumber");
    const PARAM_NAMES: [&'static str; 1] = ["block_number"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (Int64,);
    type Ok = Uint64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (block_number,): Self::Params,
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
        Ok(Uint64(count as _))
    }
}

pub enum EthGetMessageCidByTransactionHash {}
impl RpcMethod<1> for EthGetMessageCidByTransactionHash {
    const NAME: &'static str = "Filecoin.EthGetMessageCidByTransactionHash";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getMessageCidByTransactionHash");
    const PARAM_NAMES: [&'static str; 1] = ["tx_hash"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthHash,);
    type Ok = Option<Cid>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (tx_hash,): Self::Params,
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
    const API_PATHS: ApiPaths = ApiPaths::V1;
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

pub enum EthEstimateGas {}

impl RpcMethod<2> for EthEstimateGas {
    const NAME: &'static str = "Filecoin.EthEstimateGas";
    const NAME_ALIAS: Option<&'static str> = Some("eth_estimateGas");
    const N_REQUIRED_PARAMS: usize = 1;
    const PARAM_NAMES: [&'static str; 2] = ["tx", "block_param"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthCallMessage, Option<BlockNumberOrHash>);
    type Ok = Uint64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (tx, block_param): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let mut msg = Message::try_from(tx)?;
        // Set the gas limit to the zero sentinel value, which makes
        // gas estimation actually run.
        msg.gas_limit = 0;
        let tsk = if let Some(block_param) = block_param {
            Some(
                tipset_by_block_number_or_hash(ctx.chain_store(), block_param)?
                    .key()
                    .clone(),
            )
        } else {
            None
        };
        match gas::estimate_message_gas(&ctx, msg, None, tsk.clone().into()).await {
            Err(e) => {
                // On failure, GasEstimateMessageGas doesn't actually return the invocation result,
                // it just returns an error. That means we can't get the revert reason.
                //
                // So we re-execute the message with EthCall (well, applyMessage which contains the
                // guts of EthCall). This will give us an ethereum specific error with revert
                // information.
                // TODO(forest): https://github.com/ChainSafe/forest/issues/4554
                Err(anyhow::anyhow!("failed to estimate gas: {e}").into())
            }
            Ok(gassed_msg) => {
                let expected_gas = Self::eth_gas_search(&ctx, gassed_msg, &tsk.into()).await?;
                Ok(expected_gas.into())
            }
        }
    }
}

impl EthEstimateGas {
    pub async fn eth_gas_search<DB>(
        data: &Ctx<DB>,
        msg: Message,
        tsk: &ApiTipsetKey,
    ) -> anyhow::Result<u64>
    where
        DB: Blockstore + Send + Sync + 'static,
    {
        let (_invoc_res, apply_ret, prior_messages, ts) =
            gas::GasEstimateGasLimit::estimate_call_with_gas(
                data,
                msg.clone(),
                tsk,
                VMTrace::Traced,
            )
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
            let ret = Self::gas_search(data, &msg, &prior_messages, ts).await?;
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
        ts: Arc<Tipset>,
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
            ts: Arc<Tipset>,
            limit: u64,
        ) -> anyhow::Result<bool>
        where
            DB: Blockstore + Send + Sync + 'static,
        {
            msg.gas_limit = limit;
            let (_invoc_res, apply_ret) = data
                .state_manager
                .call_with_gas(
                    &mut msg.into(),
                    prior_messages,
                    Some(ts),
                    VMTrace::NotTraced,
                )
                .await?;
            Ok(apply_ret.msg_receipt().exit_code().is_success())
        }

        while high <= BLOCK_GAS_LIMIT {
            if can_succeed(data, msg.clone(), prior_messages, ts.clone(), high).await? {
                break;
            }
            low = high;
            high = high.saturating_mul(2).min(BLOCK_GAS_LIMIT);
        }

        let mut check_threshold = high / 100;
        while (high - low) > check_threshold {
            let median = (high + low) / 2;
            if can_succeed(data, msg.clone(), prior_messages, ts.clone(), high).await? {
                high = median;
            } else {
                low = median;
            }
            check_threshold = median / 100;
        }

        Ok(high)
    }
}

pub enum EthFeeHistory {}

impl RpcMethod<3> for EthFeeHistory {
    const NAME: &'static str = "Filecoin.EthFeeHistory";
    const NAME_ALIAS: Option<&'static str> = Some("eth_feeHistory");
    const N_REQUIRED_PARAMS: usize = 2;
    const PARAM_NAMES: [&'static str; 3] =
        ["block_count", "newest_block_number", "reward_percentiles"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (Uint64, BlockNumberOrPredefined, Option<Vec<f64>>);
    type Ok = EthFeeHistoryResult;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (Uint64(block_count), newest_block_number, reward_percentiles): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        if block_count > 1024 {
            return Err(anyhow::anyhow!("block count should be smaller than 1024").into());
        }

        let reward_percentiles = reward_percentiles.unwrap_or_default();
        Self::validate_reward_precentiles(&reward_percentiles)?;

        let tipset = tipset_by_block_number_or_hash(ctx.chain_store(), newest_block_number.into())?;
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
            .chain_arc(ctx.store())
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
                Self::calculate_rewards_and_gas_used(&reward_percentiles, tx_gas_rewards);
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
            oldest_block: Uint64(oldest_block_height as _),
            base_fee_per_gas: base_fee_array,
            gas_used_ratio: gas_used_ratio_array,
            reward: if reward_percentiles.is_empty() {
                None
            } else {
                Some(rewards_array)
            },
        })
    }
}

impl EthFeeHistory {
    fn validate_reward_precentiles(reward_percentiles: &[f64]) -> anyhow::Result<()> {
        if reward_percentiles.len() > 100 {
            anyhow::bail!("length of the reward percentile array cannot be greater than 100");
        }

        for (&rp, &rp_prev) in reward_percentiles
            .iter()
            .zip(std::iter::once(&0.).chain(reward_percentiles.iter()))
        {
            if !(0. ..=100.).contains(&rp) {
                anyhow::bail!("invalid reward percentile: {rp} should be between 0 and 100");
            }
            if rp < rp_prev {
                anyhow::bail!("invalid reward percentile: {rp} should be larger than {rp_prev}");
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
}

pub enum EthGetCode {}
impl RpcMethod<2> for EthGetCode {
    const NAME: &'static str = "Filecoin.EthGetCode";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getCode");
    const PARAM_NAMES: [&'static str; 2] = ["eth_address", "block_number_or_hash"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthAddress, BlockNumberOrHash);
    type Ok = EthBytes;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (eth_address, block_number_or_hash): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = tipset_by_block_number_or_hash(ctx.chain_store(), block_number_or_hash)?;
        let to_address = FilecoinAddress::try_from(&eth_address)?;
        let actor = ctx
            .state_manager
            .get_required_actor(&to_address, *ts.parent_state())?;
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
            for ts in ts.chain_arc(ctx.store()) {
                match ctx.state_manager.call(&message, Some(ts)) {
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
        if !api_invoc_result.error.is_empty() {
            return Err(anyhow::anyhow!("GetBytecode failed: {}", api_invoc_result.error).into());
        }

        let get_bytecode_return: GetBytecodeReturn =
            fvm_ipld_encoding::from_slice(msg_rct.return_data().as_slice())?;
        if let Some(cid) = get_bytecode_return.0 {
            Ok(EthBytes(ctx.store().get_required(&cid)?))
        } else {
            Ok(Default::default())
        }
    }
}

pub enum EthGetStorageAt {}
impl RpcMethod<3> for EthGetStorageAt {
    const NAME: &'static str = "Filecoin.EthGetStorageAt";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getStorageAt");
    const PARAM_NAMES: [&'static str; 3] = ["eth_address", "position", "block_number_or_hash"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthAddress, EthBytes, BlockNumberOrHash);
    type Ok = EthBytes;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (eth_address, position, block_number_or_hash): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let make_empty_result = || EthBytes(vec![0; EVM_WORD_LENGTH]);

        let ts = tipset_by_block_number_or_hash(ctx.chain_store(), block_number_or_hash)?;
        let to_address = FilecoinAddress::try_from(&eth_address)?;
        let Some(actor) = ctx
            .state_manager
            .get_actor(&to_address, *ts.parent_state())?
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
            for ts in ts.chain_arc(ctx.store()) {
                match ctx.state_manager.call(&message, Some(ts)) {
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
                "failed to lookup storage slot: {}",
                api_invoc_result.error
            )
            .into());
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
}

pub enum EthGetTransactionCount {}
impl RpcMethod<2> for EthGetTransactionCount {
    const NAME: &'static str = "Filecoin.EthGetTransactionCount";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getTransactionCount");
    const PARAM_NAMES: [&'static str; 2] = ["sender", "block_param"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthAddress, BlockNumberOrHash);
    type Ok = Uint64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (sender, block_param): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let addr = sender.to_filecoin_address()?;
        let ts = tipset_by_block_number_or_hash(ctx.chain_store(), block_param)?;
        let state = StateTree::new_from_root(ctx.store_owned(), ts.parent_state())?;
        let actor = state.get_required_actor(&addr)?;
        if is_evm_actor(&actor.code) {
            let evm_state =
                fil_actor_interface::evm::State::load(ctx.store(), actor.code, actor.state)?;
            if !evm_state.is_alive() {
                return Ok(Uint64(0));
            }

            Ok(Uint64(evm_state.nonce()))
        } else {
            Ok(Uint64(ctx.mpool.get_sequence(&addr)?))
        }
    }
}

pub enum EthMaxPriorityFeePerGas {}
impl RpcMethod<0> for EthMaxPriorityFeePerGas {
    const NAME: &'static str = "Filecoin.EthMaxPriorityFeePerGas";
    const NAME_ALIAS: Option<&'static str> = Some("eth_maxPriorityFeePerGas");
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = EthBigInt;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        match crate::rpc::gas::estimate_gas_premium(&ctx, 0).await {
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
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = Uint64;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let epoch = ctx.chain_store().heaviest_tipset().epoch();
        let version = u32::from(ctx.state_manager.get_network_version(epoch).0);
        Ok(Uint64(version.into()))
    }
}

pub enum EthGetTransactionByHash {}
impl RpcMethod<1> for EthGetTransactionByHash {
    const NAME: &'static str = "Filecoin.EthGetTransactionByHash";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getTransactionByHash");
    const PARAM_NAMES: [&'static str; 1] = ["tx_hash"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthHash,);
    type Ok = Option<ApiEthTx>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (tx_hash,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let message_cid = ctx.chain_store().get_mapping(&tx_hash)?.unwrap_or_else(|| {
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
            .search_for_message(None, message_cid, None, Some(true))
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

            if let Ok(tx) = new_eth_tx_from_message_lookup(&ctx, &message_lookup, None) {
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
}

pub enum EthGetTransactionHashByCid {}
impl RpcMethod<1> for EthGetTransactionHashByCid {
    const NAME: &'static str = "Filecoin.EthGetTransactionHashByCid";
    const NAME_ALIAS: Option<&'static str> = Some("eth_getTransactionHashByCid");
    const PARAM_NAMES: [&'static str; 1] = ["cid"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (Cid,);
    type Ok = Option<EthHash>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (cid,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let smsgs_result: Result<Vec<SignedMessage>, crate::chain::Error> =
            crate::chain::messages_from_cids(ctx.store(), &[cid]);
        if let Ok(smsgs) = smsgs_result {
            if let Some(smsg) = smsgs.first() {
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
    const N_REQUIRED_PARAMS: usize = 1;
    const PARAM_NAMES: [&'static str; 2] = ["tx", "block_param"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;
    type Params = (EthCallMessage, BlockNumberOrHash);
    type Ok = EthBytes;
    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (tx, block_param): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let msg = tx.try_into()?;
        let ts = tipset_by_block_number_or_hash(ctx.chain_store(), block_param)?;
        let invoke_result = ctx.state_manager.call(&msg, Some(ts))?;

        if msg.to() == FilecoinAddress::ETHEREUM_ACCOUNT_MANAGER_ACTOR {
            Ok(EthBytes::default())
        } else {
            let msg_rct = invoke_result.msg_rct.context("no message receipt")?;

            let bytes = decode_payload(&msg_rct.return_data(), CBOR)?;
            Ok(bytes)
        }
    }
}

pub enum EthNewFilter {}
impl RpcMethod<1> for EthNewFilter {
    const NAME: &'static str = "Filecoin.EthNewFilter";
    const NAME_ALIAS: Option<&'static str> = Some("eth_newFilter");
    const PARAM_NAMES: [&'static str; 1] = ["filter_spec"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;

    type Params = (EthFilterSpec,);
    type Ok = FilterID;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (filter_spec,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let eth_event_handler = ctx.eth_event_handler.clone();
        let chain_height = ctx.chain_store().heaviest_tipset().epoch();
        Ok(eth_event_handler
            .eth_new_filter(&filter_spec, chain_height)
            .map_err(|e| anyhow::anyhow!("{:?}", e))?)
    }
}

pub enum EthAddressToFilecoinAddress {}
impl RpcMethod<1> for EthAddressToFilecoinAddress {
    const NAME: &'static str = "Filecoin.EthAddressToFilecoinAddress";
    const NAME_ALIAS: Option<&'static str> = None;
    const N_REQUIRED_PARAMS: usize = 1;
    const PARAM_NAMES: [&'static str; 1] = ["eth_address"];
    const API_PATHS: ApiPaths = ApiPaths::V1;
    const PERMISSION: Permission = Permission::Read;
    type Params = (EthAddress,);
    type Ok = FilecoinAddress;
    async fn handle(
        _ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (eth_address,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        Ok(eth_address.to_filecoin_address()?)
    }
}

#[cfg(test)]
mod test {
    use super::*;
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
}

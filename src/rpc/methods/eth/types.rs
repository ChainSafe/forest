// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::rpc::eth::pubsub_trait::LogFilter;
use anyhow::ensure;
use get_size2::GetSize;
use ipld_core::serde::SerdeError;
use jsonrpsee::core::traits::IdProvider;
use jsonrpsee::types::SubscriptionId;
use libsecp256k1::util::FULL_PUBLIC_KEY_SIZE;
use rand::Rng;
use serde::de::{IntoDeserializer, value::StringDeserializer};
use std::collections::BTreeMap;
use std::{hash::Hash, ops::Deref};

pub const METHOD_GET_BYTE_CODE: u64 = 3;
pub const METHOD_GET_STORAGE_AT: u64 = 5;

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
    derive_more::Deref,
    GetSize,
)]
pub struct EthBytes(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::hexify_vec_bytes")]
    pub Vec<u8>,
);
lotus_json_with_self!(EthBytes);

impl From<RawBytes> for EthBytes {
    fn from(value: RawBytes) -> Self {
        Self(value.into())
    }
}

impl From<Bloom> for EthBytes {
    fn from(value: Bloom) -> Self {
        Self(value.0.0.to_vec())
    }
}

impl FromStr for EthBytes {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let deserializer: StringDeserializer<SerdeError> = String::from_str(s)?.into_deserializer();
        let bytes = crate::lotus_json::hexify_vec_bytes::deserialize(deserializer)?;
        Ok(Self(bytes))
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GetBytecodeReturn(pub Option<Cid>);

const GET_STORAGE_AT_PARAMS_ARRAY_LENGTH: usize = 32;
const LENGTH_BUF_GET_STORAGE_AT_PARAMS: u8 = 129;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetStorageAtParams(pub [u8; GET_STORAGE_AT_PARAMS_ARRAY_LENGTH]);

impl GetStorageAtParams {
    pub fn new(position: Vec<u8>) -> anyhow::Result<Self> {
        if position.len() > GET_STORAGE_AT_PARAMS_ARRAY_LENGTH {
            anyhow::bail!("supplied storage key is too long");
        }
        let mut bytes = [0; GET_STORAGE_AT_PARAMS_ARRAY_LENGTH];
        bytes
            .get_mut(GET_STORAGE_AT_PARAMS_ARRAY_LENGTH.saturating_sub(position.len())..)
            .expect("Infallible")
            .copy_from_slice(&position);
        Ok(Self(bytes))
    }

    pub fn serialize_params(&self) -> anyhow::Result<Vec<u8>> {
        let mut encoded = vec![LENGTH_BUF_GET_STORAGE_AT_PARAMS];
        fvm_ipld_encoding::to_writer(&mut encoded, &RawBytes::new(self.0.to_vec()))?;
        Ok(encoded)
    }

    pub fn deserialize_params(bz: &[u8]) -> anyhow::Result<Self> {
        let (&prefix, bytes) = bz.split_first().context("unexpected EOF")?;
        ensure!(
            prefix == LENGTH_BUF_GET_STORAGE_AT_PARAMS,
            "expected CBOR array of length 1"
        );
        let decoded: RawBytes = fvm_ipld_encoding::from_slice(bytes)?;
        GetStorageAtParams::new(decoded.into())
    }
}

#[derive(
    Eq,
    Hash,
    PartialEq,
    PartialOrd,
    Ord,
    Debug,
    Deserialize,
    Serialize,
    Default,
    Clone,
    Copy,
    JsonSchema,
    derive_more::From,
    derive_more::Into,
    derive_more::FromStr,
)]
pub struct EthAddress(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::hexify_bytes")]
    pub ethereum_types::Address,
);
lotus_json_with_self!(EthAddress);

impl GetSize for EthAddress {
    fn get_heap_size(&self) -> usize {
        0
    }
}

impl EthAddress {
    pub fn to_filecoin_address(self) -> anyhow::Result<FilecoinAddress> {
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
    pub fn from_filecoin_address(addr: &FilecoinAddress) -> anyhow::Result<Self> {
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
                    let eth_addr: EthAddress = f4_addr.subaddress().try_into()?;
                    if eth_addr.is_masked_id() {
                        bail!(
                            "f410f addresses cannot embed masked-ID payloads: {}",
                            eth_addr.0
                        );
                    }
                    return Ok(eth_addr);
                }
                bail!("invalid delegated address namespace in: {addr}")
            }
            _ => {
                bail!("invalid address {addr}");
            }
        }
    }

    pub fn is_masked_id(&self) -> bool {
        self.0.as_bytes().starts_with(&MASKED_ID_PREFIX)
    }

    pub fn from_actor_id(id: u64) -> Self {
        let pfx = MASKED_ID_PREFIX;
        let arr = id.to_be_bytes();
        let payload = [
            pfx[0], pfx[1], pfx[2], pfx[3], pfx[4], pfx[5], pfx[6], pfx[7], //
            pfx[8], pfx[9], pfx[10], pfx[11], //
            arr[0], arr[1], arr[2], arr[3], arr[4], arr[5], arr[6], arr[7],
        ];

        Self(ethereum_types::H160(payload))
    }

    /// Returns the Ethereum address corresponding to an uncompressed secp256k1 public key.
    pub fn eth_address_from_pub_key(pubkey: &[u8]) -> anyhow::Result<Self> {
        // Check if the public key has the correct length (65 bytes)
        ensure!(
            pubkey.len() == FULL_PUBLIC_KEY_SIZE,
            "uncompressed public key should have {} bytes, but got {}",
            FULL_PUBLIC_KEY_SIZE,
            pubkey.len()
        );

        // Check if the first byte of the public key is 0x04 (uncompressed)
        ensure!(
            *pubkey.first().context("failed to get pubkey prefix")? == 0x04,
            "expected first byte of uncompressed secp256k1 to be 0x04"
        );

        let hash = keccak_hash::keccak(pubkey.get(1..).context("failed to get pubkey data")?);
        let addr: &[u8] = &hash[12..32];
        EthAddress::try_from(addr)
    }
}

impl TryFrom<&[u8]> for EthAddress {
    type Error = anyhow::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != ADDRESS_LENGTH {
            bail!("cannot parse bytes into an Ethereum address: incorrect input length")
        }
        let mut payload = ethereum_types::H160::default();
        payload.as_bytes_mut().copy_from_slice(value);
        Ok(EthAddress(payload))
    }
}

impl TryFrom<&FilecoinAddress> for EthAddress {
    type Error = anyhow::Error;

    fn try_from(value: &FilecoinAddress) -> Result<Self, Self::Error> {
        Self::from_filecoin_address(value)
    }
}

impl TryFrom<FilecoinAddress> for EthAddress {
    type Error = anyhow::Error;

    fn try_from(value: FilecoinAddress) -> Result<Self, Self::Error> {
        Self::from_filecoin_address(&value)
    }
}

impl From<[u8; 20]> for EthAddress {
    fn from(value: [u8; 20]) -> Self {
        Self(ethereum_types::H160(value))
    }
}

impl TryFrom<&EthAddress> for FilecoinAddress {
    type Error = anyhow::Error;

    fn try_from(value: &EthAddress) -> Result<Self, Self::Error> {
        value.to_filecoin_address()
    }
}

impl TryFrom<EthAddress> for FilecoinAddress {
    type Error = anyhow::Error;

    fn try_from(value: EthAddress) -> Result<Self, Self::Error> {
        value.to_filecoin_address()
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema, derive_more::From)]
#[serde(untagged)]
pub enum BlockNumberOrPredefined {
    #[schemars(with = "String")]
    PredefinedBlock(Predefined),
    BlockNumber(EthInt64),
}
lotus_json_with_self!(BlockNumberOrPredefined);

impl From<BlockNumberOrPredefined> for BlockNumberOrHash {
    fn from(value: BlockNumberOrPredefined) -> Self {
        match value {
            BlockNumberOrPredefined::PredefinedBlock(v) => BlockNumberOrHash::PredefinedBlock(v),
            BlockNumberOrPredefined::BlockNumber(v) => BlockNumberOrHash::BlockNumber(v),
        }
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthFeeHistoryResult {
    pub oldest_block: EthUint64,
    pub base_fee_per_gas: Vec<EthBigInt>,
    pub gas_used_ratio: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reward: Option<Vec<Vec<EthBigInt>>>,
}
lotus_json_with_self!(EthFeeHistoryResult);

#[derive(PartialEq, Debug, Clone)]
pub struct GasReward {
    pub gas_used: u64,
    pub premium: TokenAmount,
}

#[derive(PartialEq, Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthCallMessage {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub from: Option<EthAddress>,
    // `to` is required as per [eth_call](https://www.quicknode.com/docs/ethereum/eth_call) documentation.
    // In the Filecoin context, though, it is optional due to special handling of the Ethereum
    // Account Manager.
    pub to: Option<EthAddress>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gas: Option<EthUint64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gas_price: Option<EthBigInt>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub value: Option<EthBigInt>,
    // Ethereum tools (cast, ethers.js, etc.) send calldata as `data`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub data: Option<EthBytes>,
    // Lotus/Filecoin clients send calldata as `input`.
    // Both are accepted; `input` takes precedence when both are present.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub input: Option<EthBytes>,
}
lotus_json_with_self!(EthCallMessage);

impl EthCallMessage {
    /// Returns the effective calldata, preferring `input` over `data` when both are set.
    // Some ethereum tools uses both `data` and `input` to represent calldata.
    pub fn effective_input(&self) -> Option<&EthBytes> {
        self.input.as_ref().or(self.data.as_ref())
    }

    pub fn convert_data_to_message_params(data: EthBytes) -> anyhow::Result<RawBytes> {
        if data.0.is_empty() {
            Ok(RawBytes::new(data.0))
        } else {
            Ok(RawBytes::new(fvm_ipld_encoding::to_vec(&RawBytes::new(
                data.0,
            ))?))
        }
    }
}

impl TryFrom<EthCallMessage> for Message {
    type Error = anyhow::Error;
    fn try_from(tx: EthCallMessage) -> Result<Self, Self::Error> {
        let from = match &tx.from {
            Some(addr) if addr != &EthAddress::default() => {
                // The from address must be translatable to an f4 address.
                let from = addr.to_filecoin_address()?;
                if from.protocol() != Protocol::Delegated {
                    anyhow::bail!("expected a class 4 address, got: {}", from.protocol());
                }
                from
            }
            _ => {
                // Send from the filecoin "system" address.
                EthAddress::default().to_filecoin_address()?
            }
        };
        let params = tx
            .effective_input()
            .cloned()
            .map(EthCallMessage::convert_data_to_message_params)
            .transpose()?
            .unwrap_or_default();
        let (to, method_num) = if let Some(to) = tx.to {
            (
                to.to_filecoin_address()?,
                EVMMethod::InvokeContract as MethodNum,
            )
        } else {
            (
                FilecoinAddress::ETHEREUM_ACCOUNT_MANAGER_ACTOR,
                EAMMethod::CreateExternal as MethodNum,
            )
        };
        Ok(Message {
            from,
            to,
            value: tx.value.unwrap_or_default().0.into(),
            method_num,
            params,
            gas_limit: BLOCK_GAS_LIMIT,
            ..Default::default()
        })
    }
}

#[derive(
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Debug,
    Deserialize,
    Serialize,
    Default,
    Clone,
    Copy,
    JsonSchema,
    derive_more::Display,
    derive_more::From,
    derive_more::Into,
    derive_more::FromStr,
)]
#[display("{_0:#x}")]
pub struct EthHash(#[schemars(with = "String")] pub ethereum_types::H256);
lotus_json_with_self!(EthHash);

impl GetSize for EthHash {
    fn get_heap_size(&self) -> usize {
        0
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash, Clone)]
pub struct FilterID(EthHash);

lotus_json_with_self!(FilterID);

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct ApiHeaders(#[serde(with = "crate::lotus_json")] pub Block);

lotus_json_with_self!(ApiHeaders);

impl FilterID {
    pub fn new() -> Result<Self, uuid::Error> {
        let raw_id = crate::utils::rand::new_uuid_v4();
        let mut id = [0u8; 32];
        id[..16].copy_from_slice(raw_id.as_bytes());
        Ok(FilterID(EthHash(ethereum_types::H256::from_slice(&id))))
    }
}

#[derive(Debug, Copy, Clone)]
pub struct RandomHexStringIdProvider {}

impl RandomHexStringIdProvider {
    pub fn new() -> Self {
        Self {}
    }
}

impl IdProvider for RandomHexStringIdProvider {
    fn next_id(&self) -> SubscriptionId<'static> {
        let mut bytes = [0u8; 32];
        let mut rng = crate::utils::rand::forest_rng();
        rng.fill(&mut bytes);

        SubscriptionId::Str(format!("{}", EthHash::from(bytes)).into())
    }
}

/// `EthHashList` represents a topic filter that can take one of two forms:
/// - `List`: Matches if the hash is present in the vector.
/// - `Single`: An optional hash, where:
///     - `Some(hash)`: Matches exactly this hash.
///     - `None`: Acts as a wildcard.
#[derive(PartialEq, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(untagged)]
pub enum EthHashList {
    List(Vec<EthHash>),
    Single(Option<EthHash>),
}

#[derive(Default, Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct EthTopicSpec(pub Vec<EthHashList>);

/// Represents an [`EthAddress`] or a collection of thereof. This allows the caller to either use,
/// e.g., `0x1234...` or `["0x1234...", "0x5678..."]` as the address parameter.
#[derive(PartialEq, Serialize, Deserialize, Debug, Clone, JsonSchema, derive_more::From)]
#[serde(untagged)]
pub enum EthAddressList {
    List(Vec<EthAddress>),
    Single(EthAddress),
}

impl Default for EthAddressList {
    fn default() -> Self {
        EthAddressList::List(Vec::new())
    }
}

impl Deref for EthAddressList {
    type Target = [EthAddress];

    fn deref(&self) -> &Self::Target {
        match self {
            EthAddressList::List(addrs) => addrs,
            EthAddressList::Single(addr) => std::slice::from_ref(addr),
        }
    }
}

/// Represents a filter specification for querying Ethereum event logs.
/// This struct can be used to specify criteria for filtering Ethereum event logs based on block range,
/// address, topics, and block hash. It is useful for making requests to Ethereum nodes to fetch logs
/// that match certain conditions.
///
/// # Fields
///
/// * `from_block` - Optional field interpreted as an epoch (in hex):
///   - `"latest"`: latest mined block.
///   - `"earliest"`: first block.
///   - `"pending"`: blocks that have not yet been mined.
///     If omitted, the default value is `"latest"`.
///     This field is skipped during serialization if `None`.
///
/// * `to_block` - Optional field interpreted as an epoch (in hex):
///   - `"latest"`: latest mined block.
///   - `"earliest"`: first block.
///   - `"pending"`: blocks that have not yet been mined.
///     If omitted, the default value is `"latest"`.
///     This field is skipped during serialization if `None`.
///
/// * `address` - Optional field interpreted as Actor address or a list of addresses (`Vec<EthAddress>`) from which event logs should originate.
///   If the filter needs to match a single address, it can be specified as a single element vector.
///   This field is required and cannot be omitted.
///
/// * `topics` - List of topics (`EthTopicSpec`) to be matched in the event logs.  
///
/// * `block_hash` - Optional field specifying a block hash (`Hash`)
///   Restricts event logs returned to those emitted from messages contained in this tipset. When `block_hash` is provided,
///   neither `from_block` nor `to_block` can be specified.
///   This field is skipped during serialization if `None`.
///   [the spec](https://github.com/filecoin-project/lotus/blob/475139ff95407ed9d55d3a2ef87e28da66512937/chain/types/ethtypes/eth_types.go#L602-L627).
#[derive(Default, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthFilterSpec {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub from_block: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub to_block: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub address: Option<EthAddressList>,
    pub topics: Option<EthTopicSpec>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub block_hash: Option<EthHash>,
}
lotus_json_with_self!(EthFilterSpec);

impl From<LogFilter> for EthFilterSpec {
    fn from(filter: LogFilter) -> Self {
        EthFilterSpec {
            from_block: None,
            to_block: None,
            block_hash: None,
            address: Some(filter.address),
            topics: filter.topics,
        }
    }
}

/// `EthFilterResult` represents the response from executing a filter:
/// - A list of block hashes
/// - A list of transaction hashes
/// - Or a list of logs
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum EthFilterResult {
    Hashes(Vec<EthHash>),
    Logs(Vec<EthLog>),
}
lotus_json_with_self!(EthFilterResult);

impl EthFilterResult {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Hashes(v) => v.is_empty(),
            Self::Logs(v) => v.is_empty(),
        }
    }
}

impl PartialEq for EthFilterResult {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Hashes(a), Self::Hashes(b)) => a == b,
            (Self::Logs(a), Self::Logs(b)) => a == b,
            _ => self.is_empty() && other.is_empty(),
        }
    }
}

#[derive(Eq, Hash, PartialEq, Default, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthCallTraceAction {
    pub call_type: String,
    pub from: EthAddress,
    pub to: Option<EthAddress>,
    pub gas: EthUint64,
    pub value: EthBigInt,
    pub input: EthBytes,
}

#[derive(Eq, Hash, PartialEq, Default, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthCreateTraceAction {
    pub from: EthAddress,
    pub gas: EthUint64,
    pub value: EthBigInt,
    pub init: EthBytes,
}

#[derive(Eq, Hash, PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum TraceAction {
    Call(EthCallTraceAction),
    Create(EthCreateTraceAction),
}

impl Default for TraceAction {
    fn default() -> Self {
        TraceAction::Call(EthCallTraceAction::default())
    }
}

#[derive(Eq, Hash, PartialEq, Default, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthCallTraceResult {
    pub gas_used: EthUint64,
    pub output: EthBytes,
}

#[derive(Eq, Hash, PartialEq, Default, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthCreateTraceResult {
    pub address: Option<EthAddress>,
    pub gas_used: EthUint64,
    pub code: EthBytes,
}

#[derive(Eq, Hash, PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum TraceResult {
    Call(EthCallTraceResult),
    Create(EthCreateTraceResult),
}

impl Default for TraceResult {
    fn default() -> Self {
        TraceResult::Call(EthCallTraceResult::default())
    }
}

/// The Available built-in tracer.
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum GethDebugBuiltInTracerType {
    /// The call tracer builds a hierarchical call tree, showing the hierarchy of calls (e.g., `call`, `create`, `reward`)
    #[serde(rename = "callTracer")]
    Call,
    /// The flat call tracer builds a flat list of all calls, showing the hierarchy of calls (e.g., `call`, `create`, `reward`)
    #[serde(rename = "flatCallTracer")]
    FlatCall,
    /// The prestate tracer builds a state snapshot of the accounts necessary to execute the transaction, and the state after the transaction.
    #[serde(rename = "prestateTracer")]
    PreState,
    /// The noop tracer does not build any traces.
    #[serde(rename = "noopTracer")]
    Noop,
}

/// Options for the `debug_traceTransaction` API.
#[derive(PartialEq, Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GethDebugTracingOptions {
    /// The tracer to use for the transaction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracer: Option<GethDebugBuiltInTracerType>,
    /// The configuration for the provided tracer.
    /// The configuration is a JSON object that is specific to the tracer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracer_config: Option<TracerConfig>,
}

lotus_json_with_self!(GethDebugTracingOptions);

impl GethDebugTracingOptions {
    /// Extracts and validates the `callTracer` config.
    /// Returns an error if an unsupported flag (e.g. `withLog`) is set to true.
    pub fn call_config(&self) -> anyhow::Result<CallTracerConfig> {
        let cfg = parse_tracer_config::<CallTracerConfig>(&self.tracer_config);
        if cfg.with_log.unwrap_or(false) {
            anyhow::bail!("callTracer: withLog is not yet supported");
        }
        Ok(cfg)
    }

    /// Extracts the `prestateTracer` config, defaulting to no-op values when absent.
    pub fn prestate_config(&self) -> PreStateConfig {
        parse_tracer_config::<PreStateConfig>(&self.tracer_config)
    }
}

/// Parses a tracer-specific config from the opaque [`TracerConfig`] JSON blob.
/// Returns `T::default()` when the config is absent or null, and logs a warning
/// if the config is present but fails to deserialize.
fn parse_tracer_config<T: Default + serde::de::DeserializeOwned>(raw: &Option<TracerConfig>) -> T {
    let Some(cfg) = raw.as_ref().filter(|c| !c.0.is_null()) else {
        return T::default();
    };
    serde_json::from_value(cfg.0.clone()).unwrap_or_else(|e| {
        tracing::warn!(
            error = %e,
            "invalid tracerConfig — using defaults"
        );
        T::default()
    })
}

/// Configuration for the `callTracer`.
#[derive(PartialEq, Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CallTracerConfig {
    /// When set to true, only the top call will be returned.
    /// Otherwise, the call tracer will return the full call tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub only_top_call: Option<bool>,
    /// When set to true, logs emitted during calls will be included in the trace.
    /// Not yet supported — a request with this flag set to true will return an error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub with_log: Option<bool>,
}

lotus_json_with_self!(CallTracerConfig);

/// Configuration for the `prestateTracer`.
// Taken from https://github.com/alloy-rs/alloy/blob/v1.5.2/crates/rpc-types-trace/src/geth/pre_state.rs#L236
#[derive(PartialEq, Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreStateConfig {
    /// When set to true, the pre and post state of the accounts will be returned in the trace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_mode: Option<bool>,
    /// When set to true, the code of the accounts will not be returned in the trace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disable_code: Option<bool>,
    /// When set to true, the storage of the accounts will not be returned in the trace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disable_storage: Option<bool>,
}

lotus_json_with_self!(PreStateConfig);

impl PreStateConfig {
    pub fn is_diff_mode(&self) -> bool {
        self.diff_mode.unwrap_or(false)
    }

    pub fn is_code_disabled(&self) -> bool {
        self.disable_code.unwrap_or(false)
    }

    pub fn is_storage_disabled(&self) -> bool {
        self.disable_storage.unwrap_or(false)
    }
}

/// Opaque JSON blob for per-tracer configuration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct TracerConfig(pub serde_json::Value);
lotus_json_with_self!(TracerConfig);

/// EVM call/create operation type for Geth-style trace frames.
///
/// Maps to the EVM opcodes: CALL, STATICCALL, DELEGATECALL, CREATE, CREATE2.
/// Used as the `type` field in [`GethCallFrame`].
#[derive(PartialEq, Eq, Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub enum GethCallType {
    #[default]
    #[serde(rename = "CALL")]
    Call,
    #[serde(rename = "STATICCALL")]
    StaticCall,
    #[serde(rename = "DELEGATECALL")]
    DelegateCall,
    #[serde(rename = "CREATE")]
    Create,
    #[serde(rename = "CREATE2")]
    Create2,
}

impl GethCallType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Call => "CALL",
            Self::StaticCall => "STATICCALL",
            Self::DelegateCall => "DELEGATECALL",
            Self::Create => "CREATE",
            Self::Create2 => "CREATE2",
        }
    }

    pub const fn is_static_call(&self) -> bool {
        matches!(self, Self::StaticCall)
    }

    /// Converts a Parity-style call type string to a [`GethCallType`].
    pub fn from_parity_call_type(call_type: &str) -> Self {
        match call_type {
            "staticcall" => Self::StaticCall,
            "delegatecall" => Self::DelegateCall,
            _ => Self::Call,
        }
    }
}

impl std::fmt::Display for GethCallType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Geth-style nested call frame returned by the `callTracer`.
#[derive(PartialEq, Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GethCallFrame {
    pub r#type: GethCallType,
    pub from: EthAddress,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<EthAddress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<EthBigInt>,
    pub gas: EthUint64,
    pub gas_used: EthUint64,
    pub input: EthBytes,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<EthBytes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revert_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calls: Option<Vec<GethCallFrame>>,
}

lotus_json_with_self!(GethCallFrame);

/// Empty frame returned by the `noopTracer`.
#[derive(PartialEq, Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct NoopFrame {}

lotus_json_with_self!(NoopFrame);

/// Snapshot of a single account's state at a point in time.
/// All fields are optional; absent means "not relevant" or "default".
// Taken from https://github.com/alloy-rs/alloy/blob/v1.5.2/crates/rpc-types-trace/src/geth/pre_state.rs#L108
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AccountState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance: Option<EthBigInt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<EthBytes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<EthUint64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub storage: BTreeMap<EthHash, EthHash>,
}

impl AccountState {
    /// Strips fields that are identical in `other`, keeping only changed ones.
    /// Used to minimize the `post` side of diff-mode output.
    pub fn retain_changed(&mut self, other: &Self) {
        if self.balance == other.balance {
            self.balance = None;
        }
        if self.nonce == other.nonce {
            self.nonce = None;
        }
        if self.code == other.code {
            self.code = None;
        }
        self.storage.retain(|k, v| other.storage.get(k) != Some(v));
    }

    pub fn is_empty(&self) -> bool {
        self.balance.is_none()
            && self.code.is_none()
            && self.nonce.is_none()
            && self.storage.is_empty()
    }
}

/// Returns the account states necessary to execute a given transaction.
// Taken from https://github.com/alloy-rs/alloy/blob/v1.5.2/crates/rpc-types-trace/src/geth/pre_state.rs#L72
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct PreStateMode(pub BTreeMap<EthAddress, AccountState>);

lotus_json_with_self!(PreStateMode);

/// Account state differences between the transaction's pre and post-state.
// Taken from https://github.com/alloy-rs/alloy/blob/v1.5.2/crates/rpc-types-trace/src/geth/pre_state.rs#L88
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DiffMode {
    /// account state before the transaction is executed.
    pub pre: BTreeMap<EthAddress, AccountState>,
    /// account state after the transaction is executed.
    pub post: BTreeMap<EthAddress, AccountState>,
}

lotus_json_with_self!(DiffMode);

/// Return type for the `prestateTracer`.
// Taken from https://github.com/alloy-rs/alloy/blob/v1.5.2/crates/rpc-types-trace/src/geth/pre_state.rs#L33
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum PreStateFrame {
    /// Default mode: returns the accounts necessary to execute a given transaction.
    Default(PreStateMode),
    /// Diff mode: returns the differences between the transaction's pre and post-state.
    Diff(DiffMode),
}

lotus_json_with_self!(PreStateFrame);

/// Tracing response objects
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum GethTrace {
    /// Response object for the call tracer.
    Call(GethCallFrame),
    /// Response object for the flat call tracer.
    FlatCall(Vec<EthBlockTrace>),
    /// Response object for the prestate tracer.
    PreState(PreStateFrame),
    /// Response object for the noop tracer.
    Noop(NoopFrame),
}

lotus_json_with_self!(GethTrace);

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

#[derive(Eq, Hash, PartialEq, Default, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthTrace {
    pub r#type: String,
    pub subtraces: i64,
    pub trace_address: Vec<i64>,
    pub action: TraceAction,
    pub result: TraceResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl EthTrace {
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    /// Returns true if the trace is a revert error.
    ///
    /// This is not a complete check for reverted traces (there are other possible revert reasons).
    pub fn is_reverted(&self) -> bool {
        self.error
            .as_deref()
            .is_some_and(|e| e == trace::PARITY_TRACE_REVERT_ERROR)
    }

    /// Converts the Parity-format error stored in this trace to the Geth-format.
    pub fn to_geth_error(&self) -> Option<String> {
        self.error.as_deref().map(|error| {
            if error == trace::PARITY_TRACE_REVERT_ERROR {
                trace::GETH_TRACE_REVERT_ERROR.into()
            } else {
                error.to_string()
            }
        })
    }
}

#[derive(Eq, Hash, PartialEq, Default, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthBlockTrace {
    #[serde(flatten)]
    pub trace: EthTrace,
    pub block_hash: EthHash,
    pub block_number: i64,
    pub transaction_hash: EthHash,
    pub transaction_position: i64,
}
lotus_json_with_self!(EthBlockTrace);

impl EthBlockTrace {
    pub fn sort_key(&self) -> (i64, i64, &[i64]) {
        (
            self.block_number,
            self.transaction_position,
            self.trace.trace_address.as_slice(),
        )
    }
}

/// Replay block transaction trace.
#[derive(PartialEq, Default, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthReplayBlockTransactionTrace {
    /// The full trace of the transaction.
    #[serde(flatten)]
    pub full_trace: EthTraceResults,
    /// The hash of the transaction.
    pub transaction_hash: EthHash,
    /// The VM trace of the transaction.
    /// This is optional because the VM trace is not always available (not supported by FVM).
    pub vm_trace: Option<String>,
}
lotus_json_with_self!(EthReplayBlockTransactionTrace);

// EthTraceFilterCriteria defines the criteria for filtering traces.
#[derive(Default, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthTraceFilterCriteria {
    /// Interpreted as an epoch (in hex) or one of "latest" for last mined block, "pending" for not yet committed messages.
    /// Optional, default: "latest".
    /// Note: "earliest" is not a permitted value.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub from_block: Option<String>,

    /// Interpreted as an epoch (in hex) or one of "latest" for last mined block, "pending" for not yet committed messages.
    /// Optional, default: "latest".
    /// Note: "earliest" is not a permitted value.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub to_block: Option<String>,

    /// Actor address or a list of addresses from which transactions that generate traces should originate.
    /// Optional, default: None.
    /// The JSON decoding must treat a string as equivalent to an array with one value, for example
    /// "0x8888f1f195afa192cfee86069858" must be decoded as [ "0x8888f1f195afa192cfee86069858" ]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub from_address: Option<EthAddressList>,

    /// Actor address or a list of addresses to which transactions that generate traces are sent.
    /// Optional, default: None.
    /// The JSON decoding must treat a string as equivalent to an array with one value, for example
    /// "0x8888f1f195afa192cfee86069858" must be decoded as [ "0x8888f1f195afa192cfee86069858" ]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub to_address: Option<EthAddressList>,

    /// After specifies the offset for pagination of trace results. The number of traces to skip before returning results.
    /// Optional, default: None.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub after: Option<EthUint64>,

    /// Limits the number of traces returned.
    /// Optional, default: all traces.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub count: Option<EthUint64>,
}
lotus_json_with_self!(EthTraceFilterCriteria);

impl EthTrace {
    pub fn match_filter_criteria(
        &self,
        from_decoded_addresses: &Option<EthAddressList>,
        to_decoded_addresses: &Option<EthAddressList>,
    ) -> Result<bool> {
        let (trace_to, trace_from) = match &self.action {
            TraceAction::Call(action) => (action.to, action.from),
            TraceAction::Create(action) => {
                let address = match &self.result {
                    TraceResult::Create(result) => result
                        .address
                        .ok_or_else(|| anyhow::anyhow!("address is nil in create trace result"))?,
                    _ => bail!("invalid create trace result"),
                };
                (Some(address), action.from)
            }
        };

        // Match FromAddress
        if let Some(from_addresses) = from_decoded_addresses
            && !from_addresses.is_empty()
            && !from_addresses.contains(&trace_from)
        {
            return Ok(false);
        }

        // Match ToAddress
        if let Some(to_addresses) = to_decoded_addresses
            && !to_addresses.is_empty()
            && !trace_to.is_some_and(|to| to_addresses.contains(&to))
        {
            return Ok(false);
        }

        Ok(true)
    }
}

/// Represents a changed value with before and after states.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChangedType<T> {
    /// Value before the change
    pub from: T,
    /// Value after the change
    pub to: T,
}

/// Represents how a value changed during transaction execution.
// Taken from https://github.com/alloy-rs/alloy/blob/v1.5.2/crates/rpc-types-trace/src/parity.rs#L84
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Delta<T> {
    /// Existing value didn't change.
    #[serde(rename = "=")]
    #[default]
    Unchanged,
    /// A new value was added (account/storage created).
    #[serde(rename = "+")]
    Added(T),
    /// The existing value was removed (account/storage deleted).
    #[serde(rename = "-")]
    Removed(T),
    /// The existing value changed from one value to another.
    #[serde(rename = "*")]
    Changed(ChangedType<T>),
}

impl<T: PartialEq> Delta<T> {
    /// Compares optional old/new values and returns the appropriate delta variant:
    /// `Unchanged` if both are equal or absent,
    /// `Added` if only new exists,
    /// `Removed` if only old exists,
    /// `Changed` if both exist but differ.
    pub fn from_comparison(old: Option<T>, new: Option<T>) -> Self {
        match (old, new) {
            (None, None) => Delta::Unchanged,
            (None, Some(new_val)) => Delta::Added(new_val),
            (Some(old_val), None) => Delta::Removed(old_val),
            (Some(old_val), Some(new_val)) => {
                if old_val == new_val {
                    Delta::Unchanged
                } else {
                    Delta::Changed(ChangedType {
                        from: old_val,
                        to: new_val,
                    })
                }
            }
        }
    }

    pub fn is_unchanged(&self) -> bool {
        matches!(self, Delta::Unchanged)
    }
}

/// Account state diff after transaction execution.
/// Tracks changes to balance, nonce, code, and storage.
// Taken from https://github.com/alloy-rs/alloy/blob/v1.5.2/crates/rpc-types-trace/src/parity.rs#L156
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AccountDiff {
    pub balance: Delta<EthBigInt>,
    pub code: Delta<EthBytes>,
    pub nonce: Delta<EthUint64>,
    /// All touched/changed storage values (key -> delta)
    pub storage: BTreeMap<EthHash, Delta<EthHash>>,
}

impl AccountDiff {
    /// Returns true if the account diff contains no changes.
    pub fn is_unchanged(&self) -> bool {
        self.balance.is_unchanged()
            && self.code.is_unchanged()
            && self.nonce.is_unchanged()
            && self.storage.is_empty()
    }
}

/// State diff containing all account changes from a transaction.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct StateDiff(pub BTreeMap<EthAddress, AccountDiff>);

impl StateDiff {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Inserts the account diff only if it contains at least one change.
    pub fn insert_if_changed(&mut self, addr: EthAddress, diff: AccountDiff) {
        if !diff.is_unchanged() {
            self.0.insert(addr, diff);
        }
    }
}

lotus_json_with_self!(StateDiff);

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, prelude::BASE64_STANDARD};

    #[test]
    fn get_bytecode_return_roundtrip() {
        let bytes = hex::decode("d82a5827000155a0e40220fa0b7a54007ba2e76d5818b6e60793fb0b8bdbe177995e1b20dcfb6873d69779").unwrap();
        let des: GetBytecodeReturn = fvm_ipld_encoding::from_slice(&bytes).unwrap();
        assert_eq!(
            des.0.unwrap().to_string(),
            "bafk2bzaced5aw6suab52fz3nlamlnzqhsp5qxc634f3zsxq3edopw2dt22lxs"
        );
        let ser = fvm_ipld_encoding::to_vec(&des).unwrap();
        assert_eq!(ser, bytes);
    }

    #[test]
    fn get_storage_at_params() {
        let param = GetStorageAtParams::new(vec![0xa]).unwrap();
        assert_eq!(
            &hex::encode(param.serialize_params().unwrap()),
            "815820000000000000000000000000000000000000000000000000000000000000000a"
        );
    }

    #[test]
    fn test_convert_data_to_message_params_empty() {
        let data = EthBytes(vec![]);
        let params = EthCallMessage::convert_data_to_message_params(data).unwrap();
        assert!(params.is_empty());
    }

    #[test]
    fn test_convert_data_to_message_params() {
        let data = EthBytes(BASE64_STANDARD.decode("RHt4g0E=").unwrap());
        let params = EthCallMessage::convert_data_to_message_params(data).unwrap();
        assert_eq!(BASE64_STANDARD.encode(&*params).as_str(), "RUR7eINB");
    }

    #[test]
    fn test_eth_address_from_pub_key() {
        // Uncompressed pub key secp256k1)
        let pubkey: [u8; FULL_PUBLIC_KEY_SIZE] = [
            4, 75, 249, 118, 22, 83, 215, 249, 252, 54, 149, 27, 253, 35, 238, 15, 229, 8, 50, 228,
            19, 137, 115, 123, 183, 243, 237, 144, 113, 41, 115, 70, 234, 174, 61, 199, 1, 81, 95,
            143, 102, 246, 176, 220, 176, 93, 241, 139, 94, 105, 141, 153, 20, 74, 35, 52, 139,
            137, 5, 220, 53, 194, 22, 85, 80,
        ];

        let expected_eth_address =
            EthAddress::from_str("0xeb1d0c87b7e33d0ab44a397b675f0897295491c2").unwrap();

        let result = EthAddress::eth_address_from_pub_key(&pubkey).unwrap();
        assert_eq!(result, expected_eth_address);
    }

    #[test]
    fn test_changed_type_serialization() {
        let changed = ChangedType {
            from: 10u64,
            to: 20u64,
        };
        let json = serde_json::to_string(&changed).unwrap();
        assert_eq!(json, r#"{"from":10,"to":20}"#);

        let deserialized: ChangedType<u64> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, changed);
    }

    #[test]
    fn test_delta_unchanged() {
        let delta: Delta<u64> = Delta::from_comparison(Some(42), Some(42));
        assert!(delta.is_unchanged());
        assert_eq!(delta, Delta::Unchanged);

        let json = serde_json::to_string(&delta).unwrap();
        assert_eq!(json, r#""=""#);
    }

    #[test]
    fn test_delta_added() {
        let delta: Delta<u64> = Delta::from_comparison(None, Some(100));
        assert!(!delta.is_unchanged());
        assert_eq!(delta, Delta::Added(100));

        let json = serde_json::to_string(&delta).unwrap();
        assert_eq!(json, r#"{"+":100}"#);
    }

    #[test]
    fn test_delta_removed() {
        let delta: Delta<u64> = Delta::from_comparison(Some(50), None);
        assert!(!delta.is_unchanged());
        assert_eq!(delta, Delta::Removed(50));

        let json = serde_json::to_string(&delta).unwrap();
        assert_eq!(json, r#"{"-":50}"#);
    }

    #[test]
    fn test_delta_changed() {
        let delta: Delta<u64> = Delta::from_comparison(Some(10), Some(20));
        assert!(!delta.is_unchanged());
        assert_eq!(delta, Delta::Changed(ChangedType { from: 10, to: 20 }));

        let json = serde_json::to_string(&delta).unwrap();
        assert_eq!(json, r#"{"*":{"from":10,"to":20}}"#);
    }

    #[test]
    fn test_delta_none_none() {
        let delta: Delta<u64> = Delta::from_comparison(None, None);
        assert!(delta.is_unchanged());
        assert_eq!(delta, Delta::Unchanged);
    }

    #[test]
    fn test_delta_deserialization() {
        let unchanged: Delta<u64> = serde_json::from_str(r#""=""#).unwrap();
        assert_eq!(unchanged, Delta::Unchanged);

        let added: Delta<u64> = serde_json::from_str(r#"{"+":42}"#).unwrap();
        assert_eq!(added, Delta::Added(42));

        let removed: Delta<u64> = serde_json::from_str(r#"{"-":42}"#).unwrap();
        assert_eq!(removed, Delta::Removed(42));

        let changed: Delta<u64> = serde_json::from_str(r#"{"*":{"from":10,"to":20}}"#).unwrap();
        assert_eq!(changed, Delta::Changed(ChangedType { from: 10, to: 20 }));
    }

    #[test]
    fn test_account_state_is_empty() {
        assert!(AccountState::default().is_empty());

        assert!(
            !AccountState {
                balance: Some(EthBigInt(BigInt::from(100))),
                ..Default::default()
            }
            .is_empty()
        );

        assert!(
            !AccountState {
                nonce: Some(EthUint64(1)),
                ..Default::default()
            }
            .is_empty()
        );

        assert!(
            !AccountState {
                code: Some(EthBytes(vec![0x60])),
                ..Default::default()
            }
            .is_empty()
        );

        let mut with_storage = AccountState::default();
        with_storage.storage.insert(
            EthHash(ethereum_types::H256::zero()),
            EthHash(ethereum_types::H256::from_low_u64_be(1)),
        );
        assert!(!with_storage.is_empty());
    }

    #[test]
    fn test_account_state_retain_changed_strips_identical_fields() {
        let pre = AccountState {
            balance: Some(EthBigInt(num::BigInt::from(1000))),
            nonce: Some(EthUint64(5)),
            code: Some(EthBytes(vec![0x60])),
            storage: BTreeMap::new(),
        };

        // Post identical to pre: everything stripped
        let mut post = pre.clone();
        post.retain_changed(&pre);
        assert!(post.is_empty());
    }

    #[test]
    fn test_account_state_retain_changed_keeps_different_fields() {
        let pre = AccountState {
            balance: Some(EthBigInt(num::BigInt::from(1000))),
            nonce: Some(EthUint64(5)),
            code: Some(EthBytes(vec![0x60])),
            storage: BTreeMap::new(),
        };

        let mut post = AccountState {
            balance: Some(EthBigInt(BigInt::from(2000))), // changed
            nonce: Some(EthUint64(5)),                    // same
            code: Some(EthBytes(vec![0x60, 0x80])),       // changed
            storage: BTreeMap::new(),
        };

        post.retain_changed(&pre);
        assert!(
            post.balance
                .is_some_and(|b| b.eq(&EthBigInt(BigInt::from(2000))))
        );
        assert!(post.nonce.is_none()); // stripped
        assert!(post.code.is_some_and(|b| b.eq(&EthBytes(vec![0x60, 0x80]))));
    }

    #[test]
    fn test_account_state_retain_changed_storage_diff() {
        let slot = EthHash(ethereum_types::H256::from_low_u64_be(1));
        let val_a = EthHash(ethereum_types::H256::from_low_u64_be(100));
        let val_b = EthHash(ethereum_types::H256::from_low_u64_be(200));

        let mut pre_storage = BTreeMap::new();
        pre_storage.insert(slot, val_a);

        let pre = AccountState {
            storage: pre_storage,
            ..Default::default()
        };

        // Same slot, same value -> stripped
        let mut post_same = AccountState {
            storage: {
                let mut m = BTreeMap::new();
                m.insert(slot, val_a);
                m
            },
            ..Default::default()
        };
        post_same.retain_changed(&pre);
        assert!(post_same.storage.is_empty());

        // Same slot, different value -> kept
        let mut post_diff = AccountState {
            storage: {
                let mut m = BTreeMap::new();
                m.insert(slot, val_b);
                m
            },
            ..Default::default()
        };
        post_diff.retain_changed(&pre);
        assert_eq!(post_diff.storage.len(), 1);
        assert_eq!(post_diff.storage[&slot], val_b);
    }

    #[test]
    fn test_account_diff_is_unchanged() {
        assert!(AccountDiff::default().is_unchanged());

        assert!(
            !AccountDiff {
                balance: Delta::Added(EthBigInt(num::BigInt::from(1))),
                ..Default::default()
            }
            .is_unchanged()
        );

        assert!(
            !AccountDiff {
                nonce: Delta::Changed(ChangedType {
                    from: EthUint64(0),
                    to: EthUint64(1),
                }),
                ..Default::default()
            }
            .is_unchanged()
        );

        let mut with_storage = AccountDiff::default();
        with_storage.storage.insert(
            EthHash(ethereum_types::H256::zero()),
            Delta::Added(EthHash(ethereum_types::H256::from_low_u64_be(1))),
        );
        assert!(!with_storage.is_unchanged());
    }

    #[test]
    fn test_state_diff_insert_if_changed() {
        let mut sd = StateDiff::new();
        let addr = EthAddress::default();

        // Unchanged diff is not inserted
        sd.insert_if_changed(addr, AccountDiff::default());
        assert!(sd.0.is_empty());

        // Changed diff is inserted
        let changed = AccountDiff {
            balance: Delta::Added(EthBigInt(num::BigInt::from(100))),
            ..Default::default()
        };
        sd.insert_if_changed(addr, changed);
        assert_eq!(sd.0.len(), 1);
    }

    #[test]
    fn test_prestate_config_defaults() {
        let cfg = PreStateConfig {
            diff_mode: None,
            disable_code: None,
            disable_storage: None,
        };
        assert!(!cfg.is_diff_mode());
        assert!(!cfg.is_code_disabled());
        assert!(!cfg.is_storage_disabled());
    }

    #[test]
    fn test_prestate_config_enabled() {
        let cfg = PreStateConfig {
            diff_mode: Some(true),
            disable_code: Some(true),
            disable_storage: Some(true),
        };
        assert!(cfg.is_diff_mode());
        assert!(cfg.is_code_disabled());
        assert!(cfg.is_storage_disabled());
    }

    #[test]
    fn test_prestate_config_explicit_false() {
        let cfg = PreStateConfig {
            diff_mode: Some(false),
            disable_code: Some(false),
            disable_storage: Some(false),
        };
        assert!(!cfg.is_diff_mode());
        assert!(!cfg.is_code_disabled());
        assert!(!cfg.is_storage_disabled());
    }

    #[test]
    fn test_geth_call_type_from_parity_call_type() {
        assert_eq!(
            GethCallType::from_parity_call_type("staticcall"),
            GethCallType::StaticCall
        );
        assert_eq!(
            GethCallType::from_parity_call_type("delegatecall"),
            GethCallType::DelegateCall
        );
        assert_eq!(
            GethCallType::from_parity_call_type("call"),
            GethCallType::Call
        );
        // Unknown types default to Call
        assert_eq!(
            GethCallType::from_parity_call_type("unknown"),
            GethCallType::Call
        );
        assert_eq!(GethCallType::from_parity_call_type(""), GethCallType::Call);
    }

    #[test]
    fn test_geth_call_type_is_static_call() {
        assert!(GethCallType::StaticCall.is_static_call());
        assert!(!GethCallType::Call.is_static_call());
        assert!(!GethCallType::DelegateCall.is_static_call());
        assert!(!GethCallType::Create.is_static_call());
        assert!(!GethCallType::Create2.is_static_call());
    }

    #[test]
    fn test_geth_call_type_serialization() {
        assert_eq!(
            serde_json::to_string(&GethCallType::Call).unwrap(),
            r#""CALL""#
        );
        assert_eq!(
            serde_json::to_string(&GethCallType::StaticCall).unwrap(),
            r#""STATICCALL""#
        );
        assert_eq!(
            serde_json::to_string(&GethCallType::DelegateCall).unwrap(),
            r#""DELEGATECALL""#
        );
        assert_eq!(
            serde_json::to_string(&GethCallType::Create).unwrap(),
            r#""CREATE""#
        );
        assert_eq!(
            serde_json::to_string(&GethCallType::Create2).unwrap(),
            r#""CREATE2""#
        );
    }
}

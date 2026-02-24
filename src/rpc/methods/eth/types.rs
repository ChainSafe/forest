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

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BlockNumberOrPredefined {
    #[schemars(with = "String")]
    PredefinedBlock(ExtPredefined),
    BlockNumber(EthInt64),
}
lotus_json_with_self!(BlockNumberOrPredefined);

impl From<BlockNumberOrPredefined> for ExtBlockNumberOrHash {
    fn from(value: BlockNumberOrPredefined) -> Self {
        match value {
            BlockNumberOrPredefined::PredefinedBlock(v) => ExtBlockNumberOrHash::PredefinedBlock(v),
            BlockNumberOrPredefined::BlockNumber(v) => ExtBlockNumberOrHash::BlockNumber(v),
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
    // Some clients use `input`, others use `data`. We have to support both.
    #[serde(alias = "input", skip_serializing_if = "Option::is_none", default)]
    pub data: Option<EthBytes>,
}
lotus_json_with_self!(EthCallMessage);

impl EthCallMessage {
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
            .data
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

#[derive(PartialEq, Default, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthReplayBlockTransactionTrace {
    pub output: EthBytes,
    pub state_diff: Option<String>,
    pub trace: Vec<EthTrace>,
    pub transaction_hash: EthHash,
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
}

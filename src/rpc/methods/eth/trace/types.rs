// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Shared type definitions for trace-related RPC responses.
//!
//! Trace actions, results, filter criteria, and state-diff primitives used
//! across the `trace_*` RPC methods.

use super::super::types::{EthAddress, EthAddressList, EthBytes, EthHash};
use super::super::{EthBigInt, EthUint64};
use crate::lotus_json::lotus_json_with_self;
use crate::rpc::eth::trace::utils::extract_revert_reason;
use crate::rpc::eth::trace::{GETH_TRACE_REVERT_ERROR, PARITY_TRACE_REVERT_ERROR};
use anyhow::{Result, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    #[serde(skip_serializing_if = "Option::is_none")]
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
        let cfg = parse_tracer_config::<CallTracerConfig>(self.tracer_config.as_ref());
        if cfg.with_log.unwrap_or(false) {
            anyhow::bail!("callTracer: withLog is not yet supported");
        }
        Ok(cfg)
    }

    /// Extracts the `prestateTracer` config, defaulting to no-op values when absent.
    pub fn prestate_config(&self) -> PreStateConfig {
        parse_tracer_config::<PreStateConfig>(self.tracer_config.as_ref())
    }
}

/// Parses a tracer-specific config from the opaque [`TracerConfig`] JSON blob.
/// Returns `T::default()` when the config is absent or null, and logs a warning
/// if the config is present but fails to deserialize.
fn parse_tracer_config<T: Default + serde::de::DeserializeOwned>(raw: Option<&TracerConfig>) -> T {
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

impl EthTraceResults {
    /// Constructs from Parity traces, extracting output from the root trace.
    pub fn from_parity_traces(traces: Vec<EthTrace>) -> Self {
        let output = traces
            .first()
            .map_or_else(EthBytes::default, |trace| match &trace.result {
                TraceResult::Call(r) => r.output.clone(),
                TraceResult::Create(r) => r.code.clone(),
            });
        Self {
            output,
            state_diff: None,
            trace: traces,
        }
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
            .is_some_and(|e| e == PARITY_TRACE_REVERT_ERROR)
    }

    /// Converts the Parity-format error stored in this trace to the Geth-format.
    pub fn to_geth_error(&self) -> Option<String> {
        self.error.as_deref().map(|error| {
            if error == PARITY_TRACE_REVERT_ERROR {
                GETH_TRACE_REVERT_ERROR.into()
            } else {
                error.to_string()
            }
        })
    }

    /// Converts a Parity-style [`EthTrace`] into a Geth-style [`GethCallFrame`].
    // Code taken from https://github.com/paradigmxyz/revm-inspectors/blob/v0.36.0/src/tracing/types.rs#L430
    pub fn into_geth_frame(self, call_type: GethCallType) -> anyhow::Result<GethCallFrame> {
        let is_success = self.is_success();
        let is_revert = self.is_reverted();
        let error = self.to_geth_error();

        match (self.action, self.result) {
            (TraceAction::Call(action), TraceResult::Call(result)) => {
                let mut frame = GethCallFrame {
                    r#type: call_type.clone(),
                    from: action.from,
                    to: action.to,
                    value: if call_type.is_static_call() {
                        None
                    } else {
                        Some(action.value)
                    },
                    gas: action.gas,
                    gas_used: result.gas_used,
                    input: action.input,
                    output: (!result.output.is_empty()).then_some(result.output.clone()),
                    error: None,
                    revert_reason: None,
                    calls: None,
                };

                if !is_success {
                    if !is_revert {
                        frame.gas_used = action.gas;
                        frame.output = None;
                    } else {
                        frame.revert_reason = extract_revert_reason(&result.output);
                    }
                    frame.error = error;
                }

                Ok(frame)
            }
            (TraceAction::Create(action), TraceResult::Create(result)) => {
                let mut frame = GethCallFrame {
                    r#type: call_type,
                    from: action.from,
                    to: result.address,
                    value: Some(action.value),
                    gas: action.gas,
                    gas_used: result.gas_used,
                    input: action.init,
                    output: (!result.code.is_empty()).then_some(result.code.clone()),
                    error: None,
                    revert_reason: None,
                    calls: None,
                };

                if !is_success {
                    frame.to = None;
                    if !is_revert {
                        frame.gas_used = action.gas;
                        frame.output = None;
                    } else {
                        frame.revert_reason = extract_revert_reason(&result.code);
                    }
                    frame.error = error;
                }
                Ok(frame)
            }
            _ => anyhow::bail!("mismatched trace action and result types"),
        }
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

#[derive(PartialEq, Default, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EthReplayBlockTransactionTrace {
    #[serde(flatten)]
    pub full_trace: EthTraceResults,
    pub transaction_hash: EthHash,
    /// `None` because FVM does not support opcode-level VM traces.
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
        from_decoded_addresses: Option<&EthAddressList>,
        to_decoded_addresses: Option<&EthAddressList>,
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
    use num_bigint::BigInt;

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

    #[test]
    fn test_eth_trace_to_geth_frame_successful_call() {
        let from = EthAddress::default();
        let to = EthAddress::from_actor_id(100);

        let trace = EthTrace {
            r#type: "call".into(),
            action: TraceAction::Call(EthCallTraceAction {
                call_type: "call".into(),
                from,
                to: Some(to),
                gas: EthUint64(21000),
                value: EthBigInt(num::BigInt::from(1000)),
                input: EthBytes(vec![0x01, 0x02]),
            }),
            result: TraceResult::Call(EthCallTraceResult {
                gas_used: EthUint64(5000),
                output: EthBytes(vec![0x03]),
            }),
            error: None,
            ..EthTrace::default()
        };

        let frame = trace.into_geth_frame(GethCallType::Call).unwrap();
        assert_eq!(frame.r#type, GethCallType::Call);
        assert_eq!(frame.from, from);
        assert_eq!(frame.to, Some(to));
        assert_eq!(frame.gas.0, 21000);
        assert_eq!(frame.gas_used.0, 5000);
        assert!(frame.error.is_none());
        assert!(frame.revert_reason.is_none());
        assert_eq!(frame.value, Some(EthBigInt(num::BigInt::from(1000))));
    }

    #[test]
    fn test_eth_trace_to_geth_frame_static_call_no_value() {
        let trace = EthTrace {
            r#type: "call".into(),
            action: TraceAction::Call(EthCallTraceAction {
                call_type: "staticcall".into(),
                from: EthAddress::default(),
                to: Some(EthAddress::from_actor_id(100)),
                gas: EthUint64(21000),
                value: EthBigInt(num::BigInt::from(0)),
                input: EthBytes(vec![]),
            }),
            result: TraceResult::Call(EthCallTraceResult {
                gas_used: EthUint64(100),
                output: EthBytes(vec![]),
            }),
            error: None,
            ..EthTrace::default()
        };

        let frame = trace.into_geth_frame(GethCallType::StaticCall).unwrap();
        // Static calls omit the value field
        assert!(frame.value.is_none());
    }

    #[test]
    fn test_eth_trace_to_geth_frame_reverted_call() {
        let trace = EthTrace {
            r#type: "call".into(),
            action: TraceAction::Call(EthCallTraceAction {
                call_type: "call".into(),
                from: EthAddress::default(),
                to: Some(EthAddress::from_actor_id(100)),
                gas: EthUint64(21000),
                value: EthBigInt(num::BigInt::from(0)),
                input: EthBytes(vec![]),
            }),
            result: TraceResult::Call(EthCallTraceResult {
                gas_used: EthUint64(100),
                output: EthBytes(vec![]),
            }),
            error: Some(PARITY_TRACE_REVERT_ERROR.into()),
            ..EthTrace::default()
        };

        let frame = trace.into_geth_frame(GethCallType::Call).unwrap();
        assert!(frame.error.is_some());
        assert_eq!(
            frame.error.as_deref(),
            Some(GETH_TRACE_REVERT_ERROR) // to_geth_error converts
        );
    }

    #[test]
    fn test_eth_trace_to_geth_frame_successful_create() {
        let from = EthAddress::default();
        let created = EthAddress::from_actor_id(200);
        let init_code = EthBytes(vec![0x60, 0x80]);
        let trace = EthTrace {
            r#type: "create".into(),
            action: TraceAction::Create(EthCreateTraceAction {
                from,
                gas: EthUint64(100000),
                value: EthBigInt(num::BigInt::from(0)),
                init: init_code.clone(),
            }),
            result: TraceResult::Create(EthCreateTraceResult {
                gas_used: EthUint64(50000),
                address: Some(created),
                code: EthBytes(vec![0xFE]),
            }),
            error: None,
            ..EthTrace::default()
        };

        let frame = trace.into_geth_frame(GethCallType::Create).unwrap();
        assert_eq!(frame.r#type, GethCallType::Create);
        assert_eq!(frame.from, from);
        assert_eq!(frame.to, Some(created));
        assert_eq!(frame.input.0, init_code.0); // initcode goes to input
        assert!(frame.error.is_none());
    }

    #[test]
    fn test_eth_trace_to_geth_frame_mismatched_action_result() {
        // Call action with Create result should fail
        let trace = EthTrace {
            r#type: "call".into(),
            action: TraceAction::Call(EthCallTraceAction {
                call_type: "call".into(),
                from: EthAddress::default(),
                to: None,
                gas: EthUint64(0),
                value: EthBigInt(num::BigInt::from(0)),
                input: EthBytes(vec![]),
            }),
            result: TraceResult::Create(EthCreateTraceResult {
                gas_used: EthUint64(0),
                address: None,
                code: EthBytes(vec![]),
            }),
            error: None,
            ..EthTrace::default()
        };

        assert!(trace.into_geth_frame(GethCallType::Call).is_err());
    }
}

// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::super::types::{EthAddress, EthAddressList, EthBytes, EthHash};
use super::super::{EthBigInt, EthUint64};
use crate::lotus_json::lotus_json_with_self;
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

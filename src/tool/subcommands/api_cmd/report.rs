// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc;
use crate::rpc::FilterList;
use ahash::HashMap;
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_duration_ms: u128,
    pub average_duration_ms: u128,
    pub min_duration_ms: u128,
    pub max_duration_ms: u128,
    pub test_count: usize,
}

// Add a helper function to calculate performance metrics
impl PerformanceMetrics {
    pub fn from_durations(durations: &[u128]) -> Option<Self> {
        if durations.is_empty() {
            return None;
        }

        let total_duration_ms = durations.iter().sum();
        let test_count = durations.len();
        let average_duration_ms = total_duration_ms / test_count as u128;
        let min_duration_ms = *durations.iter().min().unwrap();
        let max_duration_ms = *durations.iter().max().unwrap();

        Some(Self {
            total_duration_ms,
            average_duration_ms,
            min_duration_ms,
            max_duration_ms,
            test_count,
        })
    }
}

/// Details about a successful test instance
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SuccessfulTest {
    /// The parameters used for this test
    pub request_params: serde_json::Value,

    /// Forest node response
    pub forest_status: String,

    /// Lotus node response  
    pub lotus_status: String,

    /// Individual test execution duration in milliseconds
    pub execution_duration_ms: u128,
}

/// Testing status for a method
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum MethodTestStatus {
    /// Method was successfully tested
    Tested {
        total_count: usize,
        success_count: usize,
        failure_count: usize,
    },
    /// Method was filtered out by configuration
    Filtered,
    /// Method exists but no tested
    NotTested,
}

/// Details about a failed test instance
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FailedTest {
    /// The parameters used for this test
    pub request_params: serde_json::Value,

    /// Forest node result
    pub forest_status: String,

    /// Lotus node result
    pub lotus_status: String,

    /// Diff between Forest and Lotus responses
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_diff: Option<String>,

    /// Individual test execution duration in milliseconds
    pub execution_duration_ms: u128,
}

/// Detailed report for a single RPC method
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MethodReport {
    /// Full RPC method name
    pub name: String,

    /// Required permission level
    pub permission: String,

    /// Current testing status
    pub status: MethodTestStatus,

    // Performance metrics (always included)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub performance: Option<PerformanceMetrics>,

    /// Details of successful test instances (only in full mode)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub success_test_params: Vec<SuccessfulTest>,

    /// Details of failed test instances
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failed_test_params: Vec<FailedTest>,
}

/// Report of all API comparison test results
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApiTestReport {
    /// timestamp of when the test execution started
    pub execution_datetime_utc: String,

    /// Total duration of the test run in seconds
    pub total_duration_secs: u64,

    /// Comprehensive report for each RPC method
    pub methods: Vec<MethodReport>,
}

pub fn initial_report(
    report_path: &Option<PathBuf>,
    filter_list: &FilterList,
) -> Option<HashMap<String, MethodReport>> {
    if report_path.is_none() {
        return None;
    }

    let all_methods = rpc::collect_rpc_method_info();

    let reports = all_methods
        .into_iter()
        .map(|(method_name, permission)| {
            let report = MethodReport {
                name: method_name.to_string(),
                permission: permission.to_string(),
                status: if !filter_list.authorize(method_name) {
                    MethodTestStatus::Filtered
                } else {
                    MethodTestStatus::NotTested
                },
                performance: None,
                success_test_params: vec![],
                failed_test_params: vec![],
            };
            (method_name.to_string(), report)
        })
        .collect();

    Some(reports)
}

/// Generate a diff between forest and lotus responses
pub fn generate_diff(forest_json: &serde_json::Value, lotus_json: &serde_json::Value) -> String {
    let forest_pretty = serde_json::to_string_pretty(forest_json).unwrap_or_default();
    let lotus_pretty = serde_json::to_string_pretty(lotus_json).unwrap_or_default();
    let diff = TextDiff::from_lines(&forest_pretty, &lotus_pretty);

    let mut diff_text = String::new();
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        diff_text.push_str(&format!("{sign}{change}"));
    }
    diff_text
}

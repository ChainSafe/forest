// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::ReportMode;
use crate::rpc;
use crate::rpc::FilterList;
use ahash::{HashMap, HashMapExt};
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use std::path::Path;
use std::time::Instant;
use tabled::{builder::Builder, settings::Style};

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
    /// Method exists but not tested
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

/// Report builder to encapsulate report generation logic
pub struct ReportBuilder {
    method_reports: HashMap<String, MethodReport>,
    method_timings: HashMap<String, Vec<u128>>,
    report_mode: ReportMode,
    start_time: Instant,
    failed_test_dumps: Vec<super::api_compare_tests::TestDump>,
}

impl ReportBuilder {
    pub fn new(filter_list: &FilterList, report_mode: ReportMode) -> Self {
        let all_methods = rpc::collect_rpc_method_info();

        let method_reports = all_methods
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

        Self {
            method_reports,
            method_timings: HashMap::new(),
            report_mode,
            start_time: Instant::now(),
            failed_test_dumps: vec![],
        }
    }

    pub fn track_test_result(
        &mut self,
        method_name: &str,
        success: bool,
        test_result: &super::api_compare_tests::TestResult,
        test_params: &serde_json::Value,
    ) {
        if let Some(report) = self.method_reports.get_mut(method_name) {
            // Update test status
            match &mut report.status {
                MethodTestStatus::NotTested | MethodTestStatus::Filtered => {
                    report.status = MethodTestStatus::Tested {
                        total_count: 1,
                        success_count: if success { 1 } else { 0 },
                        failure_count: if success { 0 } else { 1 },
                    };
                }
                MethodTestStatus::Tested {
                    total_count,
                    success_count,
                    failure_count,
                    ..
                } => {
                    *total_count += 1;
                    if success {
                        *success_count += 1;
                    } else {
                        *failure_count += 1;
                    }
                }
            }

            // Track timing
            self.method_timings
                .entry(method_name.to_string())
                .or_default()
                .push(test_result.duration.as_millis());

            // if there is no test result for the current method, we can skip this test
            if test_result.test_dump.is_none() {
                return;
            }

            if !success {
                let test_dump = test_result.test_dump.as_ref().unwrap();
                self.failed_test_dumps.push(test_dump.clone());
            }

            let test_dump = test_result.test_dump.as_ref().unwrap();

            // Add test details based on mode and success
            if success && matches!(self.report_mode, ReportMode::Full) {
                if let (Ok(_), Ok(_)) = (&test_dump.forest_response, &test_dump.lotus_response) {
                    report.success_test_params.push(SuccessfulTest {
                        request_params: test_params.clone(),
                        forest_status: format!("{:?}", test_result.forest_status),
                        lotus_status: format!("{:?}", test_result.lotus_status),
                        execution_duration_ms: test_result.duration.as_millis(),
                    });
                }
            } else if !success
                && matches!(self.report_mode, ReportMode::Full | ReportMode::FailureOnly)
            {
                let response_diff = match (&test_dump.forest_response, &test_dump.lotus_response) {
                    (Ok(forest_json), Ok(lotus_json)) => {
                        Some(generate_diff(forest_json, lotus_json))
                    }
                    _ => None,
                };

                report.failed_test_params.push(FailedTest {
                    request_params: test_params.clone(),
                    forest_status: format!("{:?}", test_result.forest_status),
                    lotus_status: format!("{:?}", test_result.lotus_status),
                    response_diff,
                    execution_duration_ms: test_result.duration.as_millis(),
                });
            }
        }
    }

    /// Check if there were any failures
    pub fn has_failures(&self) -> bool {
        self.method_reports.values().any(|report| {
            matches!(
                report.status,
                MethodTestStatus::Tested { failure_count, .. } if failure_count > 0
            )
        })
    }

    /// Print a summary of test results
    pub fn print_summary(&mut self) {
        // Calculate performance metrics for each method before printing
        for (method_name, timings) in &self.method_timings {
            if let Some(report) = self.method_reports.get_mut(method_name) {
                report.performance = PerformanceMetrics::from_durations(timings);
            }
        }

        let mut builder = Builder::default();
        builder.push_record(["RPC Method", "Forest", "Lotus", "Status"]);

        let mut methods: Vec<&MethodReport> = self.method_reports.values().collect();
        methods.sort_by(|a, b| a.name.cmp(&b.name));

        for report in methods {
            match &report.status {
                MethodTestStatus::Tested {
                    total_count,
                    success_count,
                    failure_count,
                } => {
                    let method_name = if *total_count > 1 {
                        format!("{} ({})", report.name, total_count)
                    } else {
                        report.name.clone()
                    };

                    let status = if *failure_count == 0 {
                        "✅ All Passed"
                    } else if *success_count == 0 {
                        "❌ All Failed"
                    } else {
                        "⚠️  Mixed Results"
                    };

                    builder.push_record([
                        method_name.as_str(),
                        &format!("{success_count}/{total_count}"),
                        &format!("{success_count}/{total_count}"),
                        status,
                    ]);
                }
                MethodTestStatus::NotTested | MethodTestStatus::Filtered => {
                    // Skip not tested and filtered methods in summary
                }
            }
        }

        let table = builder.build().with(Style::markdown()).to_string();
        println!("\n{table}");

        // Print overall summary
        let total_methods = self.method_reports.len();
        let tested_methods = self
            .method_reports
            .values()
            .filter(|r| matches!(r.status, MethodTestStatus::Tested { .. }))
            .count();
        let failed_methods = self
            .method_reports
            .values()
            .filter(|r| {
                matches!(
                    r.status,
                    MethodTestStatus::Tested { failure_count, .. } if failure_count > 0
                )
            })
            .count();

        println!("\n📊 Test Summary:");
        println!("  Total methods: {total_methods}");
        println!("  Tested methods: {tested_methods}");
        println!("  Failed methods: {failed_methods}");
        println!("  Duration: {}s", self.start_time.elapsed().as_secs());
    }

    /// Finalize and save the report in the provided directory
    pub fn finalize_and_save(mut self, report_dir: &Path) -> anyhow::Result<()> {
        // Calculate performance metrics for each method
        for (method_name, timings) in self.method_timings {
            if let Some(report) = self.method_reports.get_mut(&method_name) {
                report.performance = PerformanceMetrics::from_durations(&timings);
            }
        }

        let mut methods: Vec<MethodReport> = self.method_reports.into_values().collect();
        methods.sort_by(|a, b| a.name.cmp(&b.name));

        let report = ApiTestReport {
            execution_datetime_utc: chrono::Utc::now().to_rfc3339(),
            total_duration_secs: self.start_time.elapsed().as_secs(),
            methods,
        };

        if !report_dir.is_dir() {
            std::fs::create_dir_all(report_dir)?;
        }

        let file_name = match self.report_mode {
            ReportMode::Full => "full_report.json",
            ReportMode::FailureOnly => "failure_report.json",
            ReportMode::Summary => "summary_report.json",
        };

        std::fs::write(
            report_dir.join(file_name),
            serde_json::to_string_pretty(&report)?,
        )?;
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_metrics_calculation() {
        let durations = vec![100, 200, 300, 400, 500];
        let metrics = PerformanceMetrics::from_durations(&durations).unwrap();

        assert_eq!(metrics.test_count, 5);
        assert_eq!(metrics.total_duration_ms, 1500);
        assert_eq!(metrics.average_duration_ms, 300);
        assert_eq!(metrics.min_duration_ms, 100);
        assert_eq!(metrics.max_duration_ms, 500);
    }

    #[test]
    fn test_performance_metrics_empty() {
        let durations: Vec<u128> = vec![];
        let metrics = PerformanceMetrics::from_durations(&durations);
        assert!(metrics.is_none());
    }

    #[test]
    fn test_performance_metrics_single_value() {
        let durations = vec![150];
        let metrics = PerformanceMetrics::from_durations(&durations).unwrap();

        assert_eq!(metrics.test_count, 1);
        assert_eq!(metrics.total_duration_ms, 150);
        assert_eq!(metrics.average_duration_ms, 150);
        assert_eq!(metrics.min_duration_ms, 150);
        assert_eq!(metrics.max_duration_ms, 150);
    }
}

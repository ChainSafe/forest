// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::ReportMode;
use crate::rpc;
use crate::rpc::{FilterList, Permission};
use crate::tool::subcommands::api_cmd::api_compare_tests::TestSummary;
use ahash::{HashMap, HashMapExt, HashSet, HashSetExt};
use chrono::{DateTime, Utc};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, DurationMilliSeconds, DurationSeconds, serde_as};
use similar::{ChangeTag, TextDiff};
use std::path::Path;
use std::time::{Duration, Instant};
use tabled::{builder::Builder, settings::Style};

/// Tracks the performance metrics for a single RPC method.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerformanceMetrics {
    #[serde_as(as = "DurationMilliSeconds<u64>")]
    total_duration_ms: Duration,

    #[serde_as(as = "DurationMilliSeconds<u64>")]
    average_duration_ms: Duration,

    #[serde_as(as = "DurationMilliSeconds<u64>")]
    min_duration_ms: Duration,

    #[serde_as(as = "DurationMilliSeconds<u64>")]
    max_duration_ms: Duration,
    test_count: usize,
}

impl PerformanceMetrics {
    pub fn from_durations(durations: &[Duration]) -> Option<Self> {
        if durations.is_empty() {
            return None;
        }

        let test_count = durations.len();
        let total_duration_ms: Duration = durations.iter().sum();
        let average_duration_ms = total_duration_ms / test_count as u32;

        Some(Self {
            total_duration_ms,
            average_duration_ms,
            min_duration_ms: *durations.iter().min().expect("durations is not empty"),
            max_duration_ms: *durations.iter().max().expect("durations is not empty"),
            test_count,
        })
    }
}

/// Details about a successful test instance
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct SuccessfulTest {
    /// The parameters used for this test
    request_params: serde_json::Value,

    /// Forest node response
    forest_status: TestSummary,

    /// Lotus node response
    lotus_status: TestSummary,

    /// Individual test execution duration in milliseconds
    #[serde_as(as = "DurationMilliSeconds<u64>")]
    execution_duration_ms: Duration,
}

/// Testing status for a method
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
enum MethodTestStatus {
    /// Method was tested
    Tested {
        total_count: usize,
        success_count: usize,
        failure_count: usize,
    },
    /// Method was filtered out by configuration
    Filtered,
    /// Method exists but was not tested
    NotTested,
}

/// Details about a failed test instance
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct FailedTest {
    /// The parameters used for this test
    pub request_params: serde_json::Value,

    /// Forest test result
    pub forest_status: TestSummary,

    /// Lotus node result
    pub lotus_status: TestSummary,

    /// Diff between Forest and Lotus responses
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_diff: Option<String>,

    /// Individual test execution duration in milliseconds
    #[serde_as(as = "DurationMilliSeconds<u64>")]
    pub execution_duration_ms: Duration,
}

/// Detailed report for a single RPC method
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct MethodReport {
    /// Full RPC method name
    name: String,

    /// Required permission level
    permission: Permission,

    /// Current testing status
    status: MethodTestStatus,

    // Performance metrics (always included)
    #[serde(skip_serializing_if = "Option::is_none")]
    performance: Option<PerformanceMetrics>,

    /// Details of successful test instances (only in full mode)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    success_test_params: Vec<SuccessfulTest>,

    /// Details of failed test instances
    #[serde(skip_serializing_if = "Vec::is_empty")]
    failed_test_params: Vec<FailedTest>,
}

/// Report of all API comparison test results
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApiTestReport {
    /// timestamp of when the test execution started
    #[serde_as(as = "DisplayFromStr")]
    execution_datetime_utc: DateTime<Utc>,

    /// Total duration of the test run in seconds
    #[serde_as(as = "DurationSeconds<u64>")]
    total_duration_secs: Duration,

    /// Comprehensive report for each RPC method
    methods: Vec<MethodReport>,
}

/// Report builder to encapsulate report generation logic
pub struct ReportBuilder {
    method_reports: HashMap<String, MethodReport>,
    method_timings: HashMap<String, Vec<Duration>>,
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
                    permission,
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
                .push(test_result.duration);

            // if there is no test result for the current method, we can skip this test
            if test_result.test_dump.is_none() {
                return;
            }

            let test_dump = test_result.test_dump.as_ref().unwrap();

            if !success {
                self.failed_test_dumps.push(test_dump.clone());
            }

            // Add test details based on mode and success
            if success && matches!(self.report_mode, ReportMode::Full) {
                if let (Ok(_), Ok(_)) = (&test_dump.forest_response, &test_dump.lotus_response) {
                    report.success_test_params.push(SuccessfulTest {
                        request_params: test_params.clone(),
                        forest_status: test_result.forest_status.clone(),
                        lotus_status: test_result.lotus_status.clone(),
                        execution_duration_ms: test_result.duration,
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
                    forest_status: test_result.forest_status.clone(),
                    lotus_status: test_result.lotus_status.clone(),
                    response_diff,
                    execution_duration_ms: test_result.duration,
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
                        "✅ All Passed".into()
                    } else {
                        let mut reasons = HashSet::new();
                        for failure in &report.failed_test_params {
                            if failure.forest_status != TestSummary::Valid {
                                reasons.insert(failure.forest_status.to_string());
                            }
                            if failure.lotus_status != TestSummary::Valid {
                                reasons.insert(failure.lotus_status.to_string());
                            }
                        }

                        let reasons_str =
                            reasons.iter().map(|s| s.as_str()).collect_vec().join(", ");

                        if *success_count == 0 {
                            format!("❌ All Failed ({reasons_str})")
                        } else {
                            format!("⚠️  Mixed Results ({reasons_str})")
                        }
                    };

                    builder.push_record([
                        method_name.as_str(),
                        &format!("{success_count}/{total_count}"),
                        &format!("{success_count}/{total_count}"),
                        &status,
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
            execution_datetime_utc: Utc::now(),
            total_duration_secs: self.start_time.elapsed(),
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
    use std::time::Duration;

    #[test]
    fn test_performance_metrics_calculation() {
        let durations = vec![
            Duration::from_millis(100),
            Duration::from_millis(200),
            Duration::from_millis(300),
            Duration::from_millis(400),
            Duration::from_millis(500),
        ];
        let metrics = PerformanceMetrics::from_durations(&durations).unwrap();

        assert_eq!(metrics.test_count, 5);
        assert_eq!(metrics.total_duration_ms.as_millis(), 1500);
        assert_eq!(metrics.average_duration_ms.as_millis(), 300);
        assert_eq!(metrics.min_duration_ms.as_millis(), 100);
        assert_eq!(metrics.max_duration_ms.as_millis(), 500);
    }

    #[test]
    fn test_performance_metrics_empty() {
        let durations: Vec<Duration> = vec![];
        let metrics = PerformanceMetrics::from_durations(&durations);
        assert!(metrics.is_none());
    }

    #[test]
    fn test_performance_metrics_single_value() {
        let durations = vec![Duration::from_millis(150)];
        let metrics = PerformanceMetrics::from_durations(&durations).unwrap();

        assert_eq!(metrics.test_count, 1);
        assert_eq!(metrics.total_duration_ms.as_millis(), 150);
        assert_eq!(metrics.average_duration_ms.as_millis(), 150);
        assert_eq!(metrics.min_duration_ms.as_millis(), 150);
        assert_eq!(metrics.max_duration_ms.as_millis(), 150);
    }
}

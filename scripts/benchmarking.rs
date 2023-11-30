#!/usr/bin/env rust-script
//! Dependencies are specified here:
//!
//! ```cargo
//! [dependencies]
//! serde_json = "1.0"
//! ```
// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// This script is used to benchmark the current changes in the Forest repository against the previous release.
// Pre-requisites: install `rust-script` using command `cargo install rust-script` and `unzip` using command `apt-get install unzip`.

use std::process::{Command, Stdio};
use std::collections::HashMap;
use serde_json::Value;
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use std::sync::mpsc;

// TODO: containerize script.

fn main() {
    let metrics = vec!["car-streaming", "forest-encoding", "graph-traversal"];

    println!("Starting benchmarking script");
    let snapshot = match_snapshot();
    println!("Compiling current branch");
    compile_current_branch();
    println!("Running benchmarks for current branch");
    run_benchmarks(metrics, snapshot);    
}

fn download_snapshot() -> String {
    // Command to fetch the latest Forest snapshot from the Filecoin network.
    // TODO: change `calibnet` to `mainnet` before merging.
    let snapshot_child = Command::new("forest-tool")
        .arg("snapshot")
        .arg("fetch")
        .arg("--chain")
        .arg("calibnet")
        .spawn()
        .expect("Failed to fetch snapshot.");
    // Wait for the snapshot fetch to complete and capture status.
    snapshot_child
        .wait_with_output()
        .expect("Failed to wait on snapshot fetch step.");
    match_snapshot()
}

fn match_snapshot() -> String {
    // Command to list all the files in the current directory.
    let ls_command = Command::new("ls")
        .output()
        .expect("ls command failed to execute");
    // Convert the bytes output to a string and split it into a vector of strings.
    let ls_output = std::str::from_utf8(&ls_command.stdout).expect("Failed to convert `ls` output to string.").split("\n").collect::<Vec<&str>>();
    // Filter the vector to only include snapshots.
    // TODO: change `calibnet` to `mainnet` before merging.
    let matching_snapshots = ls_output.iter().filter(|s| s.contains("forest_snapshot_calibnet")).collect::<Vec<&&str>>();
    // `ls` automatically sorts the snapshots, so take the last one (most recent); if none exists, download one.
    let snapshot = match matching_snapshots.last() {
        Some(snapshot) => {
            println!("Matching snapshot found. Using snapshot: {}", snapshot);
            snapshot.to_string()
        },
        None => {
            println!("No matching snapshot found. Downloading snapshot");
            download_snapshot()
        },
    };
    snapshot
}

// TODO: extend this to take current PR branch as input and compile current PR branch
fn compile_current_branch() {
    // Command to compile the current branch of Forest.
    let compile_child = Command::new("cargo")
        .arg("build")
        .spawn()
        .expect("Failed to compile current branch.");

    // Wait for the compilation to complete and capture status.
    compile_child
        .wait_with_output()
        .expect("Failed to wait on compilation step.");
}

fn run_benchmarks<'a>(metrics: Vec<&str>, snapshot: String) -> HashMap<String, (String, String)> {
    let mut metrics_table: HashMap<String, (String, String)> = HashMap::new();
    metrics.iter().for_each(|s| {
        let output = generic_benchmark(s.to_string(), snapshot.clone());
        let metrics = format_output_string(output);
        metrics_table.insert(s.to_string(), (metrics.first().unwrap().to_string(), metrics.last().unwrap().to_string()));
    });
    println!("metrics table: {:?}", metrics_table);
    metrics_table
}

fn generic_benchmark(benchmark: String, snapshot: String) -> String {
    // TODO: may need to change `gtime` to `time` before merging. May also need to modify logic 
    // based on deployment. `gtime` writes output to `stderr`, so we need to pipe
    // `stderr` to capture the output there.
    let tool_child = Command::new("gtime")
        .arg("-f")
        .arg("\"%E %M\"")
        .arg("forest-tool")
        .arg("benchmark")
        .arg(benchmark.clone())
        .arg(snapshot)
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|_| panic!("Failed to run {} benchmark.", benchmark));
    let output = tool_child
        .wait_with_output()
        .unwrap_or_else(|_| panic!("Failed to wait on {} benchmark step.", benchmark));
    std::str::from_utf8(&output.stderr).expect("Failed to convert benchmark output to string.").to_string()
}

// The parsed string contains leading/trailing quotes and a trailing newline character that need to be removed.
fn format_output_string<'a>(unformatted_string: String) -> Vec<String> {
    let unformatted_string = unformatted_string.strip_prefix("\"").unwrap();
    let formatted_string = unformatted_string.strip_suffix("\"\n").unwrap();
    formatted_string.split(" ").collect::<Vec<&str>>().iter().map(|s| s.to_string()).collect()
}

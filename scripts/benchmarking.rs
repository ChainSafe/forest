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
// Pre-requisites: install `rust-script` using command `cargo install rust-script` and `unzip` using command `apt-get install unzip`

use std::process::{Command, Stdio};
use std::collections::HashMap;
use serde_json::Value;

// TODO: download files to temp folder and clean up folder after script completes.

fn main() {
    println!("Starting benchmarking script");
    let snapshot = match_snapshot();
    println!("Compiling current branch");
    compile_current_branch();
    println!("Running benchmarks for current branch");
    //run_benchmarks(snapshot);
    println!("Fetching latest binary");
    fetch_latest_binary();
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

fn run_benchmarks<'a>(snapshot: String) -> HashMap<String, (String, String)> {
    let metrics_table: HashMap<String, (String, String)> = HashMap::new();
    // TODO: create array of metrics and iterate over it to clean this section up.
    let car_streaming_output = generic_benchmark("car-streaming".to_string(), snapshot.clone());
    let car_streaming_metrics = format_output_string(car_streaming_output);
    let metrics_table = write_to_metrics_table("car-streaming".to_string(), metrics_table, (car_streaming_metrics.first().unwrap().to_string(), car_streaming_metrics.last().unwrap().to_string()));
    let forest_encoding_output = generic_benchmark("forest-encoding".to_string(), snapshot.clone());
    let forest_encoding_metrics = format_output_string(forest_encoding_output);
    let metrics_table = write_to_metrics_table("forest-encoding".to_string(), metrics_table, (forest_encoding_metrics.first().unwrap().to_string(), forest_encoding_metrics.last().unwrap().to_string()));
    let graph_traversal_output = generic_benchmark("graph-traversal".to_string(), snapshot.clone());
    let graph_traversal_metrics = format_output_string(graph_traversal_output);
    let metrics_table = write_to_metrics_table("graph-traversal".to_string(), metrics_table, (graph_traversal_metrics.first().unwrap().to_string(), graph_traversal_metrics.last().unwrap().to_string()));
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

fn write_to_metrics_table<'a>(benchmark: String, mut table: HashMap<String, (String, String)>, metrics: (String, String)) -> HashMap<String, (String, String)> {
    table.insert(benchmark, metrics);
    table
}

fn fetch_latest_binary() {
    // Return a list of releases which are parsed into JSON and filtered.
    let list_assets = Command::new("curl")
        .arg("-L")
        .arg("-H")
        .arg("Accept: application/vnd.github+json")
        .arg("-H")
        .arg("X-GitHub-Api-Version: 2022-11-28")
        .arg("https://api.github.com/repos/ChainSafe/forest/releases/latest")
        .output()
        .expect("Failed to fetch asset list.");
    let input = std::str::from_utf8(&list_assets.stdout).expect("Failed to convert `curl` output to string.");
    let value: Value = serde_json::from_str(input).unwrap();
    
    // Capture appropriate download URL and binary name for use later. Also trim leading/trailing double quotes before passing as arguments to commands.
    let mut download_url = String::new();
    let mut name = String::new();
    for i in 0..value.as_object().unwrap().len() {
        if let Some(val) = value.as_object().unwrap().get("assets").unwrap().get(i) {
            // TODO: change to Linux binary before merging
            if val.as_object().unwrap().get("browser_download_url").unwrap().to_string().contains("macos") {
                download_url = val.as_object().unwrap().get("browser_download_url").unwrap().to_string();
                name = val.as_object().unwrap().get("name").unwrap().to_string();
            }
        }
    }
    let download_url = download_url.strip_prefix("\"").unwrap();
    let download_url = download_url.strip_suffix("\"").unwrap();
    let name = name.strip_prefix("\"").unwrap();
    let name = name.strip_suffix("\"").unwrap();

    println!("Downloading latest binary {} from: {}", name, download_url);
    // Download the compressed binaries and unzip them.
    // TODO?: retry on error?
    let _download_zip_file = Command::new("wget")
        .arg(download_url)
        .output()
        .expect("Failed to fetch compressed binaries.");
    let _unzip_command = Command::new("unzip")
        .arg(name)
        .output()
        .expect("Failed to unzip binaries.");
}

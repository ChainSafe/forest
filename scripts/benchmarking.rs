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
// Pre-requisites: install `rust-script` using command `cargo install rust-script` and `unzip` using command `apt-get install unzip`. Install `docker`.

use std::process::{Command, Stdio};
use std::collections::HashMap;
use serde_json::Value;
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use std::sync::mpsc;

// TODO: download files to temp folder and clean up folder after script completes.

const ONE_MIN: Duration = Duration::from_secs(60);
const TEN_SEC: Duration = Duration::from_secs(10);

fn main() {
    let (tx, rx) = mpsc::channel();
    println!("Starting benchmarking script");
    // let snapshot = match_snapshot();
    // println!("Compiling current branch");
    // compile_current_branch();
    // println!("Running benchmarks for current branch");
    //run_benchmarks(snapshot);

    println!("Initializing Docker with latest release");
    thread::spawn(|| {initialize_docker()});

    sleep(TEN_SEC);
    println!("Fetching latest snapshot");
    thread::spawn(|| {docker_fetch_snapshot()});
    sleep(ONE_MIN);
    sleep(ONE_MIN);
    sleep(ONE_MIN);
    thread::spawn(move || {
        let snapshot = docker_match_snapshot();
        tx.send(snapshot).expect("Failed to send snapshot to main thread.");
    });
    let snapshot = rx.recv().expect("Failed to receive snapshot from snapshot match thread.");
    //let snapshot = Arc::new(snapshot);
    let mut docker_metrics_read = false;
    while !docker_metrics_read {
        let snapshot = snapshot.clone();
        println!("Attempting to get docker metrics.");
        thread::spawn(move || {try_to_get_docker_metrics(snapshot)});
        sleep(TEN_SEC);
        docker_metrics_read = true;
    }

    //initialize_docker();
    // We need to stop the container if it runs for more than __ minutes.
    // TODO: set proper time limit.
    sleep(ONE_MIN);
    println!("Maximum time limit reached. Stopping Docker container.");
    let stop_handle = thread::spawn(|| {stop_docker_container()});
    stop_handle.join().unwrap();
    //let stop_handle = thread::spawn(|| {stop_docker_container()});
    // let kill_handle = thread::spawn(|| {Command::new("docker").arg("kill").arg("forest").spawn().expect("Failed to kill docker container.")});
    // // let snapshot_handle = thread::spawn(|| {fetch_docker_snapshot()});
    // docker_handle.join().unwrap();
    
    // let mut docker_metrics_read = false;
    
}

// fn download_snapshot() -> String {
//     // Command to fetch the latest Forest snapshot from the Filecoin network.
//     // TODO: change `calibnet` to `mainnet` before merging.
//     let snapshot_child = Command::new("forest-tool")
//         .arg("snapshot")
//         .arg("fetch")
//         .arg("--chain")
//         .arg("calibnet")
//         .spawn()
//         .expect("Failed to fetch snapshot.");
//     // Wait for the snapshot fetch to complete and capture status.
//     snapshot_child
//         .wait_with_output()
//         .expect("Failed to wait on snapshot fetch step.");
//     match_snapshot()
// }

// fn match_snapshot() -> String {
//     // Command to list all the files in the current directory.
//     let ls_command = Command::new("ls")
//         .output()
//         .expect("ls command failed to execute");
//     // Convert the bytes output to a string and split it into a vector of strings.
//     let ls_output = std::str::from_utf8(&ls_command.stdout).expect("Failed to convert `ls` output to string.").split("\n").collect::<Vec<&str>>();
//     // Filter the vector to only include snapshots.
//     // TODO: change `calibnet` to `mainnet` before merging.
//     let matching_snapshots = ls_output.iter().filter(|s| s.contains("forest_snapshot_calibnet")).collect::<Vec<&&str>>();
//     // `ls` automatically sorts the snapshots, so take the last one (most recent); if none exists, download one.
//     let snapshot = match matching_snapshots.last() {
//         Some(snapshot) => {
//             println!("Matching snapshot found. Using snapshot: {}", snapshot);
//             snapshot.to_string()
//         },
//         None => {
//             println!("No matching snapshot found. Downloading snapshot");
//             download_snapshot()
//         },
//     };
//     snapshot
// }

// // TODO: extend this to take current PR branch as input and compile current PR branch
// fn compile_current_branch() {
//     // Command to compile the current branch of Forest.
//     let compile_child = Command::new("cargo")
//         .arg("build")
//         .spawn()
//         .expect("Failed to compile current branch.");

//     // Wait for the compilation to complete and capture status.
//     compile_child
//         .wait_with_output()
//         .expect("Failed to wait on compilation step.");
// }

// fn run_benchmarks<'a>(snapshot: String) -> HashMap<String, (String, String)> {
//     let metrics_table: HashMap<String, (String, String)> = HashMap::new();
//     // TODO: create array of metrics and iterate over it to clean this section up.
//     let car_streaming_output = generic_benchmark("car-streaming".to_string(), snapshot.clone());
//     let car_streaming_metrics = format_output_string(car_streaming_output);
//     let metrics_table = write_to_metrics_table("car-streaming".to_string(), metrics_table, (car_streaming_metrics.first().unwrap().to_string(), car_streaming_metrics.last().unwrap().to_string()));
//     let forest_encoding_output = generic_benchmark("forest-encoding".to_string(), snapshot.clone());
//     let forest_encoding_metrics = format_output_string(forest_encoding_output);
//     let metrics_table = write_to_metrics_table("forest-encoding".to_string(), metrics_table, (forest_encoding_metrics.first().unwrap().to_string(), forest_encoding_metrics.last().unwrap().to_string()));
//     let graph_traversal_output = generic_benchmark("graph-traversal".to_string(), snapshot.clone());
//     let graph_traversal_metrics = format_output_string(graph_traversal_output);
//     let metrics_table = write_to_metrics_table("graph-traversal".to_string(), metrics_table, (graph_traversal_metrics.first().unwrap().to_string(), graph_traversal_metrics.last().unwrap().to_string()));
//     println!("metrics table: {:?}", metrics_table);
//     metrics_table
// }

// fn generic_benchmark(benchmark: String, snapshot: String) -> String {
//     // TODO: may need to change `gtime` to `time` before merging. May also need to modify logic 
//     // based on deployment. `gtime` writes output to `stderr`, so we need to pipe
//     // `stderr` to capture the output there.
//     let tool_child = Command::new("gtime")
//         .arg("-f")
//         .arg("\"%E %M\"")
//         .arg("forest-tool")
//         .arg("benchmark")
//         .arg(benchmark.clone())
//         .arg(snapshot)
//         .stderr(Stdio::piped())
//         .spawn()
//         .unwrap_or_else(|_| panic!("Failed to run {} benchmark.", benchmark));
//     let output = tool_child
//         .wait_with_output()
//         .unwrap_or_else(|_| panic!("Failed to wait on {} benchmark step.", benchmark));
//     std::str::from_utf8(&output.stderr).expect("Failed to convert benchmark output to string.").to_string()
// }

// // The parsed string contains leading/trailing quotes and a trailing newline character that need to be removed.
// fn format_output_string<'a>(unformatted_string: String) -> Vec<String> {
//     let unformatted_string = unformatted_string.strip_prefix("\"").unwrap();
//     let formatted_string = unformatted_string.strip_suffix("\"\n").unwrap();
//     formatted_string.split(" ").collect::<Vec<&str>>().iter().map(|s| s.to_string()).collect()
// }

// fn write_to_metrics_table<'a>(benchmark: String, mut table: HashMap<String, (String, String)>, metrics: (String, String)) -> HashMap<String, (String, String)> {
//     table.insert(benchmark, metrics);
//     table
// }

fn initialize_docker() {
    // TODO: change `calibnet` to `mainnet` before merging.
    let docker_child = Command::new("docker")
        .arg("run")
        .arg("--init")
        .arg("-it")
        .arg("--rm")
        .arg("--name")
        .arg("forest")
        .arg("ghcr.io/chainsafe/forest:latest")
        .arg("--chain")
        .arg("calibnet")
        .arg("--auto-download-snapshot")
        .arg("--encrypt-keystore")
        .arg("false")
        .spawn()
        .expect("Failed to initialize docker container.");
    docker_child
        .wait_with_output()
        .expect("Failed to wait on docker initialization step.");
}

fn stop_docker_container() {
    let stop_command = Command::new("docker")
        .arg("container")
        .arg("stop")
        .arg("forest")
        .spawn()
        .expect("Failed to stop docker container.");
    stop_command
        .wait_with_output()
        .expect("Failed to wait on docker stop step.");
}

fn try_to_get_docker_metrics(snapshot: String) {
    let metrics_command = Command::new("docker")
        .arg("exec")
        .arg("-it")
        .arg("forest")
        .arg("forest-tool")
        .arg("benchmark")
        .arg("car-streaming")
        .arg(snapshot)
        .spawn()
        .expect("Failed to run benchmarking tool.");
    metrics_command
        .wait_with_output()
        .expect("Failed to wait on benchmarking step.");
}

fn docker_fetch_snapshot() {
    // TODO: change `calibnet` to `mainnet` before merging.
    let snapshot_command = Command::new("docker")
        .arg("exec")
        .arg("-it")
        .arg("forest")
        .arg("forest-tool")
        .arg("snapshot")
        .arg("fetch")
        .arg("--chain")
        .arg("calibnet")
        .spawn()
        .expect("Failed to fetch snapshot.");
    snapshot_command
        .wait_with_output()
        .expect("Failed to wait on snapshot fetch step.");
}

fn docker_match_snapshot() -> String {
    // Command to list all the files in the current directory.
    let ls_command = Command::new("docker")
        .arg("exec")
        .arg("forest")
        .arg("ls")
        .output()
        .expect("ls command failed to execute");
    dbg!(&ls_command);
    // Convert the bytes output to a string and split it into a vector of strings.
    let ls_output = std::str::from_utf8(&ls_command.stdout).expect("Failed to convert `ls` output to string.").split("\n").collect::<Vec<&str>>();
    dbg!(&ls_output);
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
            docker_fetch_snapshot();
            docker_match_snapshot()
        },
    };
    snapshot
}

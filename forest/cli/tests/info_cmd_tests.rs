// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use assert_cmd::Command;

#[test]
fn test_forest_info_cmd() {
    let cmd = Command::cargo_bin("forest-cli")
        .unwrap()
        .arg("info")
        .arg("show")
        .assert()
        .success();

    let output = std::str::from_utf8(&cmd.get_output().stdout).unwrap();
    let lines = output.split("\n").filter(|e| !e.is_empty());
    for info in lines {
        if info.starts_with("Network") {
            let info = info.split(":").skip(1).next().unwrap().trim_start();
            assert!(info == "calibnet" || info == "mainnet" || info == "devnet");
        }
        if info.starts_with("Uptime")
            || info.starts_with("Chain health")
            || info.starts_with("Chain")
            || info.starts_with("Default wallet address")
        {
            let info = info.split(":").skip(1).next().expect().trim_start();

            assert!(!info.is_empty());
        }
    }
}

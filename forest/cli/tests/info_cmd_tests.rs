use std::time::Duration;

// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use assert_cmd::Command;

#[test]
#[ignore]
fn test_forest_info_cmd() {
    let mut a = std::process::Command::new("forest")
        .arg("--chain")
        .arg("calibnet")
        .arg("--encrypt-keystore")
        .arg("false")
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(5000));
    Command::cargo_bin("forest-cli")
        .unwrap()
        .arg("info")
        .arg("show")
        .assert()
        .success();
    std::thread::sleep(Duration::from_millis(5000));
    a.kill().unwrap();
}

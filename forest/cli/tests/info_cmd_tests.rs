// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use assert_cmd::Command;

#[test]
#[ignore] // being run at scripts/forest_cli_check.sh for now as it needs the daemon to be running.
fn test_forest_info_cmd() {
    Command::cargo_bin("forest-cli")
        .unwrap()
        .arg("info")
        .arg("show")
        .assert()
        .success();
}

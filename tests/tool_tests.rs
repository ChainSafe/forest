// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod common;

use std::io::Write;

use crate::common::tool;

// Exporting an empty archive should fail but not panic
#[test]
fn export_empty_archive() {
    let mut temp_file = tempfile::NamedTempFile::new().unwrap();
    temp_file.write_all(&[]).unwrap();
    let output = tool()
        .unwrap()
        .arg("archive")
        .arg("export")
        .arg(temp_file.path())
        .assert()
        .failure();
    assert_eq!(
        std::str::from_utf8(&output.get_output().stderr).unwrap(),
        "Error: input not recognized as any kind of CAR data (.car, .car.zst, .forest.car)\n"
    )
}

// Exporting an empty archive should fail but not panic
#[test]
fn state_migration_actor_bundle_runs() {
    tool()
        .unwrap()
        .arg("state-migration")
        .arg("actor-bundle")
        .assert()
        .success();
}

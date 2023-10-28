// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod common;

use predicates::prelude::*;

use crate::common::tool;

// Exporting an empty archive should fail but not panic
#[test]
fn export_empty_archive() {
    let temp_file = tempfile::Builder::new()
        .tempfile()
        .unwrap()
        .into_temp_path();
    tool()
        .arg("archive")
        .arg("export")
        .arg(&temp_file)
        .assert()
        .failure()
        .stderr(predicate::eq(
            "Error: input not recognized as any kind of CAR data (.car, .car.zst, .forest.car)\n",
        ));
}

// Running `forest-tool state-migration actor-bundle` may not fail.
#[test]
fn state_migration_actor_bundle_runs() {
    let temp_dir = tempfile::tempdir().unwrap();
    let bundle = temp_dir.path().join("bundle.car");

    tool()
        .arg("state-migration")
        .arg("actor-bundle")
        .arg(&bundle)
        .assert()
        .success();

    assert!(bundle.exists());
    assert!(zstd::decode_all(std::fs::File::open(&bundle).unwrap()).is_ok());
}

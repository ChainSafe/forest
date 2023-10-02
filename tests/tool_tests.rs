// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod common;

use predicates::prelude::*;

use crate::common::tool;

// Exporting an empty archive should fail but not panic
#[test]
fn export_empty_archive() {
    let temp_file = tempfile::NamedTempFile::new_in(".").unwrap();
    tool()
        .arg("archive")
        .arg("export")
        .arg(temp_file.path())
        .assert()
        .failure()
        .stderr(predicate::eq(
            "Error: input not recognized as any kind of CAR data (.car, .car.zst, .forest.car)\n",
        ));
}

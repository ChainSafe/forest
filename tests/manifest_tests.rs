// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod common;

use crate::common::tool;
use std::path::PathBuf;

#[test]
fn create_manifest_json() {
    // This downloads lots of bundles from Github, which may be down at the time
    // of local development. If GH is down, the CI will likely fail as well.
    if std::env::var("CI").is_err() {
        return;
    }

    let json = tool()
        .arg("state-migration")
        .arg("generate-actors-metadata")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = String::from_utf8(json).unwrap();

    // check if the JSON is valid
    let _ = serde_json::from_str::<serde_json::Value>(&json).unwrap();

    let manifest_path = PathBuf::from("build/manifest.json");
    let manifest = std::fs::read_to_string(manifest_path).unwrap();

    // This should fail either if:
    // - the bundle list was updated and the manifest was not (this is ok, just update the manifest),
    // - the manifest generation is non-deterministic (this is bad),
    // - an existing bundle was updated under the same tag (this is bad, it should be immutable).
    assert_eq!(json, manifest);
}

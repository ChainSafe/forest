// Copyright 2019-2024 ChainSafe Systems
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

#[test]
fn peer_id_from_keypair() {
    let temp_dir = tempfile::tempdir().unwrap();
    let keypair = libp2p::identity::ed25519::Keypair::generate();

    let keypair_file = temp_dir.path().join("keypair");
    std::fs::write(&keypair_file, keypair.to_bytes()).unwrap();

    let keypair: libp2p::identity::Keypair = keypair.into();
    let expected_peer_id = keypair.public().to_peer_id().to_string();

    tool()
        .arg("shed")
        .arg("peer-id-from-key-pair")
        .arg(&keypair_file)
        .assert()
        .success()
        .stdout(format!("{expected_peer_id}\n"));

    tool()
        .arg("shed")
        .arg("peer-id-from-key-pair")
        .arg(temp_dir.path().join("azathoth"))
        .assert()
        .failure();
}

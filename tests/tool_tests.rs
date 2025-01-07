// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod common;

use common::{daemon, CommonArgs};
use predicates::prelude::*;

use crate::common::{create_tmp_config, tool};

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

#[test]
fn backup_tool_roundtrip_all() {
    let (config_file, data_dir) = create_tmp_config();

    // Create a pantheon of Old Gods in the data directory
    let old_gods = data_dir.path().join("old_gods");
    std::fs::create_dir(&old_gods).unwrap();
    let gods = vec![
        (
            "cthulhu",
            "ph'nglui mglw'nafh Cthulhu R'lyeh wgah'nagl fhtagn",
        ),
        ("azathoth", "Azathoth is the blind idiot god"),
        ("nyarlathotep", "Nyarlathotep is the crawling chaos"),
    ];

    for (name, chant) in &gods {
        std::fs::write(old_gods.join(name), chant).unwrap();
    }

    let backup_file = tempfile::Builder::new().suffix(".tar").tempfile().unwrap();

    tool()
        .arg("backup")
        .arg("create")
        .arg("--all")
        .arg("--daemon-config")
        .arg(&config_file)
        .arg("--backup-file")
        .arg(backup_file.path())
        .assert()
        .success();

    // remove the old gods
    std::fs::remove_dir_all(&old_gods).unwrap();

    tool()
        .arg("backup")
        .arg("restore")
        .arg("--force")
        .arg("--daemon-config")
        .arg(&config_file)
        .arg(backup_file.path())
        .assert()
        .success();

    assert!(old_gods.exists());
    for (name, chant) in gods {
        assert_eq!(std::fs::read_to_string(old_gods.join(name)).unwrap(), chant);
    }
}

#[test]
fn backup_tool_roundtrip_keys() {
    let (config_file, data_dir) = create_tmp_config();
    daemon()
        .common_args()
        .arg("--config")
        .arg(&config_file)
        .arg("--encrypt-keystore")
        .arg("false")
        .assert()
        .success();

    let backup_file = tempfile::Builder::new().suffix(".tar").tempfile().unwrap();
    let keypair = std::fs::read(data_dir.path().join("libp2p").join("keypair")).unwrap();
    let keystore = std::fs::read(data_dir.path().join("keystore.json")).unwrap();

    tool()
        .arg("backup")
        .arg("create")
        .arg("--daemon-config")
        .arg(&config_file)
        .arg("--backup-file")
        .arg(backup_file.path())
        .assert()
        .success();

    std::fs::remove_file(data_dir.path().join("libp2p").join("keypair")).unwrap();
    std::fs::remove_file(data_dir.path().join("keystore.json")).unwrap();

    tool()
        .arg("backup")
        .arg("restore")
        .arg("--force")
        .arg("--daemon-config")
        .arg(&config_file)
        .arg(backup_file.path())
        .assert()
        .success();

    assert_eq!(
        std::fs::read(data_dir.path().join("libp2p").join("keypair")).unwrap(),
        keypair
    );
    assert_eq!(
        std::fs::read(data_dir.path().join("keystore.json")).unwrap(),
        keystore
    );
}

#[test]
fn keypair_conversion_roundtrip() {
    // start the daemon to generate a keypair
    let (config_file, data_dir) = create_tmp_config();
    daemon()
        .common_args()
        .arg("--config")
        .arg(&config_file)
        .arg("--encrypt-keystore")
        .arg("false")
        .assert()
        .success();

    let encoded_private_key = String::from_utf8(
        tool()
            .arg("shed")
            .arg("private-key-from-key-pair")
            .arg(data_dir.path().join("libp2p").join("keypair"))
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap()
    .trim()
    .to_string();

    let keypair_file = tempfile::Builder::new()
        .suffix(".keypair")
        .tempfile()
        .unwrap();
    tool()
        .arg("shed")
        .arg("key-pair-from-private-key")
        .arg(encoded_private_key.clone())
        .arg("--output")
        .arg(keypair_file.path())
        .assert()
        .success();

    let keypair = std::fs::read(keypair_file.path()).unwrap();
    assert_eq!(
        std::fs::read(data_dir.path().join("libp2p").join("keypair")).unwrap(),
        keypair
    );

    let keypair_encoded = String::from_utf8(
        tool()
            .arg("shed")
            .arg("key-pair-from-private-key")
            .arg(encoded_private_key)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap()
    .trim()
    .to_string();

    use base64::{prelude::BASE64_STANDARD, Engine};
    let keypair_decoded = BASE64_STANDARD.decode(keypair_encoded).unwrap();
    assert_eq!(keypair, keypair_decoded);
}

#[test]
fn shed_openrpc_doesnt_crash() {
    let stdout = tool()
        .args(["shed", "openrpc", "--path", "v1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice::<openrpc_types::OpenRPC>(&stdout).unwrap();
}

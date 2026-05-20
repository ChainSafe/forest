// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Calibnet wallet integration tests. Each test assumes:
//! - `forest-wallet` is on `PATH`,
//! - a Forest daemon is running and synced to calibnet,
//! - [`FOREST_TEST_PRELOADED_ADDRESS`] is funded and imported into both backends (env var of the same name; see `forest_wallet_init`),
//! - `FULLNODE_API_INFO` is exported.

#[path = "common/calibnet_wallet_helpers.rs"]
mod helpers;

use helpers::*;
use rstest::rstest;
use serde_json::json;

#[rstest]
#[case::local(Backend::Local)]
#[case::remote(Backend::Remote)]
#[tokio::test]
async fn export_import_roundtrip(#[case] backend: Backend) {
    let addr = wallet(backend, &["new"]).unwrap();
    let exported = export_to_temp_file(&addr, backend).unwrap();
    let path = exported
        .path()
        .to_str()
        .expect("temp path is not valid UTF-8");

    let deleted = wallet(backend, &["delete", &addr]).unwrap();
    eprintln!("delete output ({}): {deleted}", backend.label());

    let imported = wallet(backend, &["import", path]).unwrap();
    assert_eq!(
        imported,
        addr,
        "round-trip mismatch on {} backend: {imported} != {addr}",
        backend.label(),
    );
}

#[tokio::test]
async fn market_add_balance_message_on_chain() {
    const ATTO_FIL: &str = "23";
    let result = rpc_call(
        "Filecoin.MarketAddBalance",
        json!([
            FOREST_TEST_PRELOADED_ADDRESS.as_str(),
            FOREST_TEST_PRELOADED_ADDRESS.as_str(),
            ATTO_FIL,
        ]),
    )
    .await
    .unwrap();
    let msg_cid = cid_from_lotus_json_result(&result).unwrap();
    poll_until_state_search_msg(&msg_cid).await.unwrap();
}

#[rstest]
#[case::local(Backend::Local)]
#[case::remote(Backend::Remote)]
#[tokio::test]
async fn send_to_filecoin_address(#[case] backend: Backend) {
    let target = wallet(backend, &["new"]).unwrap();
    let msg = send_from(&FOREST_TEST_PRELOADED_ADDRESS, &target, FIL_AMT, backend).unwrap();
    eprintln!("send to {target} ({}) msg: {msg}", backend.label());
    let funded = poll_until_funded(&target, backend).await.unwrap();
    eprintln!("{target} funded balance: {funded}");
}

#[rstest]
#[case::local(Backend::Local)]
#[case::remote(Backend::Remote)]
#[tokio::test]
async fn send_to_eth_equivalent(#[case] backend: Backend) {
    let target = wallet(backend, &["new"]).unwrap();
    let initial_msg = send_from(&FOREST_TEST_PRELOADED_ADDRESS, &target, FIL_AMT, backend).unwrap();
    eprintln!(
        "initial send to {target} ({}) msg: {initial_msg}",
        backend.label(),
    );
    let baseline = poll_until_funded(&target, backend).await.unwrap();

    let eth = filecoin_to_eth(&target).await.unwrap();
    let eth_msg = send_from(&FOREST_TEST_PRELOADED_ADDRESS, &eth, FIL_AMT, backend).unwrap();
    eprintln!("send to ETH {eth} (mapped from {target}) msg: {eth_msg}");

    let updated = poll_until_changed(&target, &baseline, backend)
        .await
        .unwrap();
    assert!(
        updated != baseline,
        "{target} balance unchanged after ETH-equivalent send: {updated}",
    );
}

#[rstest]
#[case::local(Backend::Local)]
#[case::remote(Backend::Remote)]
#[tokio::test]
async fn wallet_delete(#[case] backend: Backend) {
    let addr = wallet(backend, &["new"]).unwrap();
    let deleted = wallet(backend, &["delete", &addr]).unwrap();
    eprintln!("delete output ({}): {deleted}", backend.label());
    let listing = wallet(backend, &["list"]).unwrap();
    assert!(
        !listing.contains(&addr),
        "deleted wallet {addr} still appears in `list`:\n{listing}",
    );
}

#[rstest]
#[case::local(Backend::Local)]
#[case::remote(Backend::Remote)]
#[tokio::test]
async fn delegated_send(#[case] target_backend: Backend) {
    let funded = funded_delegated_addr().await;
    let target = wallet(target_backend, &["new", "delegated"]).unwrap();
    // Baseline `FIL_ZERO` ⇒ first credit; otherwise expect a balance delta.
    let baseline = balance(&target, target_backend).unwrap();
    let msg = send_from(funded, &target, FIL_AMT, Backend::Local).unwrap();
    eprintln!(
        "delegated send to {target} ({}) msg: {msg}",
        target_backend.label(),
    );
    let observed = if baseline == FIL_ZERO {
        poll_until_funded(&target, target_backend).await.unwrap()
    } else {
        poll_until_changed(&target, &baseline, target_backend)
            .await
            .unwrap()
    };
    assert!(
        observed != baseline,
        "{target} balance unchanged after delegated send: {observed}",
    );
}

#[tokio::test]
async fn delegated_remote_send() {
    let funded = funded_delegated_addr().await;
    let target = wallet(Backend::Remote, &["new", "delegated"]).unwrap();
    let baseline = balance(&target, Backend::Remote).unwrap();
    let msg = send_from(funded, &target, FIL_AMT, Backend::Remote).unwrap();
    eprintln!("delegated --remote-wallet send to {target} msg: {msg}");
    let observed = if baseline == FIL_ZERO {
        poll_until_funded(&target, Backend::Remote).await.unwrap()
    } else {
        poll_until_changed(&target, &baseline, Backend::Remote)
            .await
            .unwrap()
    };
    assert!(
        observed != baseline,
        "{target} balance unchanged after delegated --remote-wallet send: {observed}",
    );
}

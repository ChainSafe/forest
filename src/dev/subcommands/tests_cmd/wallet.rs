// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::helpers::*;
use libtest_mimic::{Arguments, Trial};

/// Wallet integration tests
#[derive(Debug, clap::Args)]
pub struct WalletTestCommand {}

impl WalletTestCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let args = Arguments {
            test_threads: Some(8),
            ..Default::default()
        };
        libtest_mimic::run(&args, tests()).exit();
    }
}

fn tests() -> Vec<Trial> {
    vec![
        Trial::test("export_import_roundtrip_local", || {
            block_on(export_import_roundtrip(Backend::Local));
            Ok(())
        }),
        Trial::test("export_import_roundtrip_remote", || {
            block_on(export_import_roundtrip(Backend::Remote));
            Ok(())
        }),
        Trial::test("market_add_balance_message_on_chain", || {
            block_on(market_add_balance_message_on_chain());
            Ok(())
        }),
        Trial::test("send_to_filecoin_address_local", || {
            block_on(send_to_filecoin_address(Backend::Local));
            Ok(())
        }),
        Trial::test("send_to_filecoin_address_remote", || {
            block_on(send_to_filecoin_address(Backend::Remote));
            Ok(())
        }),
        Trial::test("send_to_eth_equivalent_local", || {
            block_on(send_to_eth_equivalent(Backend::Local));
            Ok(())
        }),
        Trial::test("send_to_eth_equivalent_remote", || {
            block_on(send_to_eth_equivalent(Backend::Remote));
            Ok(())
        }),
        Trial::test("wallet_delete_local", || {
            block_on(wallet_delete(Backend::Local));
            Ok(())
        }),
        Trial::test("wallet_delete_remote", || {
            block_on(wallet_delete(Backend::Remote));
            Ok(())
        }),
        Trial::test("delegated_send_local", || {
            block_on(delegated_send(Backend::Local));
            Ok(())
        }),
        Trial::test("delegated_send_remote", || {
            block_on(delegated_send(Backend::Remote));
            Ok(())
        }),
        Trial::test("delegated_remote_send", || {
            block_on(delegated_remote_send());
            Ok(())
        }),
    ]
}

async fn export_import_roundtrip(backend: Backend) {
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

async fn market_add_balance_message_on_chain() {
    const ATTO_FIL: &str = "23";
    let result = rpc_call(
        "Filecoin.MarketAddBalance",
        serde_json::json!([
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

async fn send_to_filecoin_address(backend: Backend) {
    let target = wallet(backend, &["new"]).unwrap();
    let msg = send_from(&FOREST_TEST_PRELOADED_ADDRESS, &target, FIL_AMT, backend).unwrap();
    eprintln!("send to {target} ({}) msg: {msg}", backend.label());
    let funded = poll_until_funded(&target, backend).await.unwrap();
    eprintln!("{target} funded balance: {funded}");
}

async fn send_to_eth_equivalent(backend: Backend) {
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

async fn wallet_delete(backend: Backend) {
    let addr = wallet(backend, &["new"]).unwrap();
    let deleted = wallet(backend, &["delete", &addr]).unwrap();
    eprintln!("delete output ({}): {deleted}", backend.label());
    let listing = wallet(backend, &["list"]).unwrap();
    assert!(
        !listing.contains(&addr),
        "deleted wallet {addr} still appears in `list`:\n{listing}",
    );
}

async fn delegated_send(target_backend: Backend) {
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

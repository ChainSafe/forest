// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Calibnet wallet integration tests. Each test assumes:
//! - `forest-wallet` and `forest-cli` are on `PATH`,
//! - a Forest daemon is running and synced to calibnet,
//! - [`FOREST_TEST_PRELOADED_ADDRESS`] is funded and imported into both backends (env var of the same name; see `forest_wallet_init`),
//! - `FULLNODE_API_INFO` is exported.

#[path = "common/calibnet_wallet_helpers.rs"]
mod helpers;

use std::io::Write as _;

use anyhow::Context as _;
use helpers::*;
use rstest::rstest;
use serde_json::json;
use tempfile::NamedTempFile;
use tokio::sync::OnceCell;

/// Test amount to be transferred between accounts in wallet tests.
const FIL_AMT: &str = "500 atto FIL";
/// Amount to seed a freshly-created delegated wallet.
const DELEGATE_FUND_AMT: &str = "3 micro FIL";

static FUNDED_DELEGATED: OnceCell<String> = OnceCell::const_new();

/// Delegated signer: create once on local, fund locally, mirror to remote.
async fn funded_delegated_addr() -> &'static str {
    let addr = FUNDED_DELEGATED
        .get_or_try_init(|| async {
            let addr = Backend::Local.run(&["new", "delegated"]).unwrap();
            let fund_msg = Backend::Local
                .send(&FOREST_TEST_PRELOADED_ADDRESS, &addr, DELEGATE_FUND_AMT)
                .unwrap();
            eprintln!("delegated funding send to {addr} msg: {fund_msg}");
            wait_for_msg(&fund_msg).await.unwrap();
            let funded = poll_until_funded(&addr, Backend::Local).await.unwrap();
            eprintln!("delegated wallet {addr} funded balance: {funded}");

            let mirrored = import_wallet(&addr, Backend::Local, Backend::Remote).unwrap();
            assert_eq!(mirrored, addr, "mirror mismatch: {mirrored} != {addr}");
            Ok::<_, anyhow::Error>(addr)
        })
        .await
        .unwrap();
    addr.as_str()
}

impl Backend {
    /// Exact on-chain balance of `address`.
    fn balance(self, address: &str) -> anyhow::Result<String> {
        self.run(&["balance", address, "--exact-balance"])
    }
}

/// Poll until the balance reported for `address` differs from `baseline`.
async fn poll_until_changed(
    address: &str,
    baseline: &str,
    backend: Backend,
) -> anyhow::Result<String> {
    let label = format!("{backend:?} balance change for {address}");
    let baseline = baseline.to_string();
    poll(&label, || async {
        let bal = backend.balance(address)?;
        Ok((bal != baseline).then_some(bal))
    })
    .await
}

/// Poll until the balance reported for `address` is no longer [`FIL_ZERO`].
async fn poll_until_funded(address: &str, backend: Backend) -> anyhow::Result<String> {
    poll_until_changed(address, FIL_ZERO, backend).await
}

/// Import an address from one backend into another.
fn import_wallet(address: &str, from: Backend, to: Backend) -> anyhow::Result<String> {
    let raw = from.run_raw(&["export", address])?;
    let mut file = NamedTempFile::new_in(std::env::temp_dir())
        .context("failed to create temp file for wallet export")?;
    file.write_all(&raw)?;
    file.flush()?;
    let path = file
        .path()
        .to_str()
        .context("temp path is not valid UTF-8")?;

    if to == from {
        to.run(&["delete", address])?;
    }
    to.run(&["import", path])
}

#[rstest]
#[case::local(Backend::Local)]
#[case::remote(Backend::Remote)]
#[tokio::test]
async fn export_import_roundtrip(#[case] backend: Backend) {
    let addr = backend.run(&["new"]).unwrap();
    let imported = import_wallet(&addr, backend, backend).unwrap();
    assert_eq!(
        imported, addr,
        "round-trip mismatch on {backend:?}: {imported} != {addr}",
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
    let msg_cid = result
        .get("/")
        .and_then(|v| v.as_str())
        .expect("MarketAddBalance should return a CID");
    wait_for_msg(msg_cid).await.unwrap();
}

#[rstest]
#[case::local(Backend::Local)]
#[case::remote(Backend::Remote)]
#[tokio::test]
async fn send_to_filecoin_address(#[case] backend: Backend) {
    let target = backend.run(&["new"]).unwrap();
    let msg = backend
        .send(&FOREST_TEST_PRELOADED_ADDRESS, &target, FIL_AMT)
        .unwrap();
    eprintln!("send to {target} ({backend:?}) msg: {msg}");
    wait_for_msg(&msg).await.unwrap();
    let funded = poll_until_funded(&target, backend).await.unwrap();
    eprintln!("{target} funded balance: {funded}");
}

#[rstest]
#[case::local(Backend::Local)]
#[case::remote(Backend::Remote)]
#[tokio::test]
async fn send_to_eth_equivalent(#[case] backend: Backend) {
    let target = backend.run(&["new"]).unwrap();
    let initial_msg = backend
        .send(&FOREST_TEST_PRELOADED_ADDRESS, &target, FIL_AMT)
        .unwrap();
    eprintln!("initial send to {target} ({backend:?}) msg: {initial_msg}");
    wait_for_msg(&initial_msg).await.unwrap();
    let baseline = poll_until_funded(&target, backend).await.unwrap();

    let eth_result = rpc_call(
        "Filecoin.FilecoinAddressToEthAddress",
        json!([&target, "pending"]),
    )
    .await
    .unwrap();
    let eth = eth_result.as_str().expect("expected string ETH address");
    let eth_msg = backend
        .send(&FOREST_TEST_PRELOADED_ADDRESS, eth, FIL_AMT)
        .unwrap();
    eprintln!("send to ETH {eth} (mapped from {target}) msg: {eth_msg}");
    wait_for_msg(&eth_msg).await.unwrap();

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
    let addr = backend.run(&["new"]).unwrap();
    let deleted = backend.run(&["delete", &addr]).unwrap();
    eprintln!("delete output ({backend:?}): {deleted}");
    let listing = backend.run(&["list"]).unwrap();
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
    let target = target_backend.run(&["new", "delegated"]).unwrap();
    // Baseline `FIL_ZERO` ⇒ first credit; otherwise expect a balance delta.
    let baseline = target_backend.balance(&target).unwrap();
    let msg = Backend::Local.send(funded, &target, FIL_AMT).unwrap();
    eprintln!("delegated send to {target} ({target_backend:?}) msg: {msg}");
    wait_for_msg(&msg).await.unwrap();
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
    let target = Backend::Remote.run(&["new", "delegated"]).unwrap();
    let baseline = Backend::Remote.balance(&target).unwrap();
    let msg = Backend::Remote.send(funded, &target, FIL_AMT).unwrap();
    eprintln!("delegated --remote-wallet send to {target} msg: {msg}");
    wait_for_msg(&msg).await.unwrap();
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

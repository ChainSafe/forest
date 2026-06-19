// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Calibnet mpool CLI integration tests (shared preloaded address).
//!
//! Run via [`calibnet_wallet_mpool`] before [`calibnet_wallet`]; see `mise test:wallet`.
//! Each test assumes the same environment as [`calibnet_wallet`].

#[path = "common/calibnet_wallet_helpers.rs"]
mod helpers;

use anyhow::Context as _;
use helpers::*;
use rstest::rstest;
use serde_json::json;
use serial_test::serial;

/// Run `forest-cli <args>` and return trimmed stdout.
fn forest_cli(args: &[&str]) -> anyhow::Result<String> {
    Ok(String::from_utf8(run_command("forest-cli", args)?)?
        .trim()
        .to_string())
}

/// Next nonce for an address
fn mpool_nonce(address: &str) -> anyhow::Result<u64> {
    let out = forest_cli(&["mpool", "nonce", address])?;
    out.parse::<u64>()
        .with_context(|| format!("invalid mpool nonce output: {out}"))
}

/// Poll until `address` has a pending message at `nonce`.
async fn poll_until_pending_nonce(address: &str, nonce: u64) -> anyhow::Result<()> {
    let label = format!("pending nonce {nonce} for {address}");
    let address = address.to_string();
    poll(&label, || async {
        let result = rpc_call("Filecoin.MpoolPending", json!([null])).await?;
        let pending = result
            .as_array()
            .with_context(|| format!("expected MpoolPending array, got {result}"))?
            .iter()
            .any(|entry| {
                let Some(msg) = entry.get("Message") else {
                    return false;
                };
                msg.get("From").and_then(|v| v.as_str()) == Some(address.as_str())
                    && msg.get("Nonce").and_then(|v| v.as_u64()) == Some(nonce)
            });
        Ok(pending.then_some(()))
    })
    .await
}

#[tokio::test]
#[serial]
async fn mpool_nonce_fix_auto_unblocks_pending() {
    let addr = FOREST_TEST_PRELOADED_ADDRESS.as_str();
    let nonce = mpool_nonce(addr).unwrap();
    // Skip one nonce so `--auto` has a gap to fill.
    let next_nonce = nonce + 1;
    forest_cli(&[
        "mpool",
        "nonce-fix",
        "--addr",
        addr,
        "--start",
        &next_nonce.to_string(),
        "--end",
        &(next_nonce + 1).to_string(),
    ])
    .unwrap();
    poll_until_pending_nonce(addr, next_nonce).await.unwrap();

    forest_cli(&["mpool", "nonce-fix", "--addr", addr, "--auto"]).unwrap();

    assert!(
        poll_until_pending_nonce(addr, nonce).await.is_ok(),
        "nonce-fix --auto should fill nonce gap at {nonce} for {addr}."
    );
}

#[rstest]
#[case::local(Backend::Local)]
#[case::remote(Backend::Remote)]
#[tokio::test]
#[serial]
async fn mpool_replace_auto_unblocks_pending(#[case] backend: Backend) {
    let addr = FOREST_TEST_PRELOADED_ADDRESS.as_str();
    let nonce = mpool_nonce(addr).unwrap();

    let cid = backend.send(addr, addr, FIL_ZERO).unwrap();
    poll_until_pending_nonce(addr, nonce).await.unwrap();

    forest_cli(&[
        "mpool",
        "replace",
        "--from",
        addr,
        "--nonce",
        &nonce.to_string(),
        "--auto",
    ])
    .unwrap();

    assert!(
        wait_for_msg(&cid).await.is_ok(),
        "mpool replace --auto should replace message {cid} from {addr} at nonce {nonce}."
    );
}

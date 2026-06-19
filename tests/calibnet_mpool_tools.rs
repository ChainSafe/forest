// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Calibnet mpool CLI integration tests (shared preloaded address).
//!
//! Run via [`calibnet_wallet_mpool`] before [`calibnet_wallet`]; see `mise test:wallet`.
//! Each test assumes the same environment as [`calibnet_wallet`].

#[path = "common/calibnet_wallet_helpers.rs"]
mod helpers;

use helpers::*;
use serial_test::serial;

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

#[tokio::test]
#[serial]
async fn mpool_replace_auto_unblocks_pending() {
    let addr = FOREST_TEST_PRELOADED_ADDRESS.as_str();
    let nonce = mpool_nonce(addr).unwrap();

    let cid = send_from(addr, addr, FIL_AMT, Backend::Local).unwrap();
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

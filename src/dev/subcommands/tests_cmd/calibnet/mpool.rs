// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Calibnet mpool CLI integration tests (shared preloaded address).
//!
//! Run via [`calibnet_wallet_mpool`] before [`calibnet_wallet`]; see `mise test:wallet`.
//! Each test assumes the same environment as [`calibnet_wallet`].

use super::helpers::*;
use libtest_mimic::{Arguments, Failed, Trial};
use std::time::Duration;

/// Calibnet mpool integration tests
#[derive(Debug, clap::Args)]
pub struct CalibnetMpoolTestCommand {}

impl CalibnetMpoolTestCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let args = Arguments {
            test_threads: Some(8),
            ..Default::default()
        };
        libtest_mimic::run(&args, tests()).exit();
    }
}

fn tests() -> Vec<Trial> {
    vec![Trial::test(
        "mpool_replace_auto_unblocks_pending",
        mpool_replace_auto_unblocks_pending,
    )]
}

fn mpool_replace_auto_unblocks_pending() -> Result<(), Failed> {
    // Retry for 3 times in case race condition happens.
    // Chance of having race condition in nonce is low as messages are broadcasted in the network pretty fast
    for i in (0..3).rev() {
        if i <= 0 {
            block_on(mpool_replace_auto_unblocks_pending_async());
            break;
        } else if std::panic::catch_unwind(|| block_on(mpool_replace_auto_unblocks_pending_async()))
            .is_err()
        {
            // Retry after 5s on error
            std::thread::sleep(Duration::from_secs(5));
        } else {
            // Succeeded
            break;
        }
    }
    Ok(())
}

async fn mpool_replace_auto_unblocks_pending_async() {
    let addr = FOREST_TEST_PRELOADED_ADDRESS.as_str();
    let nonce = mpool_nonce(addr).unwrap();

    let cid = send_from_no_wait(addr, addr, FIL_AMT, Backend::Local).unwrap();
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
        poll_until_state_search_msg(&cid).await.is_ok(),
        "mpool replace --auto should replace message {cid} from {addr} at nonce {nonce}."
    );
}

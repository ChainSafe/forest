// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Basic wallet integration test.

use anyhow::{Context as _, bail};
use forest::interop_tests_private::shim::crypto::SignatureType;
use tracing::info;

use crate::{
    Backend, WalletHarness, encode_exported_key, parse_amount, parse_exported_key,
    wait_balance_above, wait_balance_nonzero,
};

/// Run the basic wallet check.
///
/// Steps:
/// 1. Pick the preloaded address from the local backend.
/// 2. Export/delete/import the key on both backends, asserting the address is
///    preserved across the round-trip.
/// 3. Create `local_new` on the local backend and `remote_new` on the remote
///    backend; set defaults; send [`crate::TRANSFER_AMOUNT`] to each.
/// 4. Poll for the recipients' balances to become non-zero.
/// 5. Resolve each address to its ETH form, send [`crate::TRANSFER_AMOUNT`]
///    to that ETH address, and poll for further balance growth.
/// 6. Smoke-test `delete` by creating a throwaway address, deleting it, and
///    asserting it is no longer listed.
///
/// Retry counts and intervals are governed by [`crate::POLL_RETRIES`] and
/// [`crate::POLL_INTERVAL`].
pub async fn run() -> anyhow::Result<()> {
    let mut harness = WalletHarness::from_env()?;

    // Commented out due to flakiness. Tracking issue:
    // https://github.com/ChainSafe/forest/issues/4849
    //
    // Begin Filecoin.MarketAddBalance test
    //
    // let market_fil_amount: TokenAmount = TokenAmount::from_atto(23);
    // let remote_addr = first_address(&harness, Backend::Remote)
    //     .await
    //     .context("expected a preloaded address in the remote keystore")?;
    // let msg_cid = MarketAddBalance::call(
    //     &harness.remote,
    //     (remote_addr, remote_addr, market_fil_amount),
    // )
    // .await
    // .context("Filecoin.MarketAddBalance failed")?;
    // info!(%msg_cid, "MarketAddBalance message submitted");
    //
    // Wait up to 30 attempts (5 tipsets) for the message to land.
    // wait_until(
    //     "wait for MarketAddBalance message",
    //     30,
    //     Duration::from_secs(5),
    //     || async {
    //         let lookup = StateSearchMsg::call(
    //             &harness.remote,
    //             (ApiTipsetKey(None), msg_cid, 800, true),
    //         )
    //         .await?;
    //         Ok(lookup.map(|_| ()))
    //     },
    // )
    // .await?;

    let fil_amount = parse_amount(crate::TRANSFER_AMOUNT)?;

    let preloaded = harness
        .preloaded_address(Backend::Local)
        .await
        .context("expected a preloaded address in the local keystore")?;
    info!(%preloaded, "selected preloaded address");

    // Pause to allow the indexer to settle
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Export / delete / import roundtrip
    let exported = harness.export(Backend::Local, preloaded).await?;
    let exported_blob = encode_exported_key(&exported)?;
    // Sanity-check the encoder by round-tripping through the parser.
    let parsed = parse_exported_key(&exported_blob)?;
    assert_eq_keys(&exported, &parsed)?;

    harness.delete(Backend::Local, preloaded).await?;
    harness.delete(Backend::Remote, preloaded).await?;

    let roundtrip_local = harness.import(Backend::Local, exported.clone()).await?;
    if roundtrip_local != preloaded {
        bail!(
            "local wallet address should be preserved across export/import (got {roundtrip_local}, expected {preloaded})"
        );
    }
    let roundtrip_remote = harness.import(Backend::Remote, exported).await?;
    if roundtrip_remote != preloaded {
        bail!(
            "remote wallet address should be preserved across export/import (got {roundtrip_remote}, expected {preloaded})"
        );
    }

    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Create new addresses and set defaults
    info!("creating local_new (local backend)");
    let local_new = harness
        .new_address(Backend::Local, SignatureType::Secp256k1)
        .await?;
    info!(%local_new, "created local_new");
    harness.set_default(Backend::Local, preloaded).await?;

    info!("creating remote_new (remote backend)");
    let remote_new = harness
        .new_address(Backend::Remote, SignatureType::Secp256k1)
        .await?;
    info!(%remote_new, "created remote_new");
    harness.set_default(Backend::Remote, preloaded).await?;

    // Initial sends to the new addresses
    let msg_local = harness
        .send(Backend::Local, local_new, fil_amount.clone())
        .await?;
    info!(%msg_local, "submitted local send to local_new");

    let msg_remote = harness
        .send(Backend::Remote, remote_new, fil_amount.clone())
        .await?;
    info!(%msg_remote, "submitted remote send to remote_new");

    let local_balance =
        wait_balance_nonzero(&harness, local_new, "wait for local_new balance").await?;
    info!(%local_new, balance = %local_balance, "local_new funded");

    let remote_balance =
        wait_balance_nonzero(&harness, remote_new, "wait for remote_new balance").await?;
    info!(%remote_new, balance = %remote_balance, "remote_new funded");

    // ETH-format sends
    let eth_local = harness.filecoin_to_eth(local_new).await?;
    info!(%local_new, ?eth_local, "resolved ETH address for local_new");
    let eth_remote = harness.filecoin_to_eth(remote_new).await?;
    info!(%remote_new, ?eth_remote, "resolved ETH address for remote_new");

    let msg_eth_local = harness
        .send_to_eth(Backend::Local, eth_local, fil_amount.clone())
        .await?;
    info!(%msg_eth_local, "submitted local send to ETH-mapped local_new");

    let msg_eth_remote = harness
        .send_to_eth(Backend::Remote, eth_remote, fil_amount.clone())
        .await?;
    info!(%msg_eth_remote, "submitted remote send to ETH-mapped remote_new");

    let after_eth_local = wait_balance_above(
        &harness,
        local_new,
        local_balance,
        "wait for local_new balance to increase post-ETH-send",
    )
    .await?;
    info!(%local_new, balance = %after_eth_local, "local_new received ETH-routed send");

    let after_eth_remote = wait_balance_above(
        &harness,
        remote_new,
        remote_balance,
        "wait for remote_new balance to increase post-ETH-send",
    )
    .await?;
    info!(%remote_new, balance = %after_eth_remote, "remote_new received ETH-routed send");

    // Final delete-and-list smoke test
    smoke_test_delete(&mut harness, Backend::Local).await?;
    smoke_test_delete(&mut harness, Backend::Remote).await?;

    // TODO: Uncomment this check once the send command is fixed.
    // The recipient's on-chain balance after the first send should equal the
    // amount sent. `TokenAmount` is already unitless atto-FIL, so we compare
    // directly without the bash `cut -d ' ' -f 1` dance.
    //
    // if local_balance != fil_amount {
    //     bail!(
    //         "FIL amount should match: local_new balance {local_balance:?}, sent {fil_amount:?}"
    //     );
    // }

    info!("basic wallet scenario completed successfully");
    Ok(())
}

fn assert_eq_keys(
    a: &forest::interop_tests_private::key_management::KeyInfo,
    b: &forest::interop_tests_private::key_management::KeyInfo,
) -> anyhow::Result<()> {
    if a == b {
        Ok(())
    } else {
        bail!("KeyInfo round-trip mismatch")
    }
}

async fn smoke_test_delete(harness: &mut WalletHarness, backend: Backend) -> anyhow::Result<()> {
    let throwaway = harness
        .new_address(backend, SignatureType::Secp256k1)
        .await?;
    info!(?backend, %throwaway, "created throwaway address for delete check");

    harness.delete(backend, throwaway).await?;

    let after = harness.list(backend).await?;
    if after.contains(&throwaway) {
        bail!(
            "{:?} backend still lists deleted address {throwaway}",
            backend
        );
    }
    Ok(())
}

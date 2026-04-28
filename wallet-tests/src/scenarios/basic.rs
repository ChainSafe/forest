// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Basic wallet integration test.

use anyhow::{Context as _, bail};
use forest::interop_tests_private::shim::{address::Address, crypto::SignatureType};
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
/// 3. Create a fresh local address (`addr_two`) and remote address
///    (`addr_three`); set defaults; send 500 atto FIL to each.
/// 4. Poll for the recipients' balances to become non-zero.
/// 5. Resolve each address to its ETH form, send 500 atto FIL to that ETH
///    address, and poll for further balance growth.
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

    let fil_amount = parse_amount("500 atto FIL")?;

    let addr_one = first_address(&harness, Backend::Local)
        .await
        .context("expected a preloaded address in the local keystore")?;
    info!(%addr_one, "selected preloaded address");

    // Pause to allow the indexer to settle
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Export / delete / import roundtrip
    let exported = harness.export(Backend::Local, addr_one).await?;
    let exported_blob = encode_exported_key(&exported)?;
    // Sanity-check the encoder by round-tripping through the parser.
    let parsed = parse_exported_key(&exported_blob)?;
    assert_eq_keys(&exported, &parsed)?;

    harness.delete(Backend::Local, addr_one).await?;
    harness.delete(Backend::Remote, addr_one).await?;

    let roundtrip_local = harness.import(Backend::Local, exported.clone()).await?;
    if roundtrip_local != addr_one {
        bail!(
            "local wallet address should be preserved across export/import (got {roundtrip_local}, expected {addr_one})"
        );
    }
    let roundtrip_remote = harness.import(Backend::Remote, exported).await?;
    if roundtrip_remote != addr_one {
        bail!(
            "remote wallet address should be preserved across export/import (got {roundtrip_remote}, expected {addr_one})"
        );
    }

    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Create new addresses and set defaults
    info!("creating addr_two locally");
    let addr_two = harness
        .new_address(Backend::Local, SignatureType::Secp256k1)
        .await?;
    info!(%addr_two, "created addr_two");
    harness.set_default(Backend::Local, addr_one).await?;

    info!("creating addr_three remotely");
    let addr_three = harness
        .new_address(Backend::Remote, SignatureType::Secp256k1)
        .await?;
    info!(%addr_three, "created addr_three");
    harness.set_default(Backend::Remote, addr_one).await?;

    // Initial sends to the new addresses
    let msg_local = harness
        .send(Backend::Local, addr_two, fil_amount.clone())
        .await?;
    info!(%msg_local, "submitted local send to addr_two");

    let msg_remote = harness
        .send(Backend::Remote, addr_three, fil_amount.clone())
        .await?;
    info!(%msg_remote, "submitted remote send to addr_three");

    let addr_two_balance =
        wait_balance_nonzero(&harness, addr_two, "wait for addr_two balance").await?;
    info!(%addr_two, balance = %addr_two_balance, "addr_two funded");

    let addr_three_balance =
        wait_balance_nonzero(&harness, addr_three, "wait for addr_three balance").await?;
    info!(%addr_three, balance = %addr_three_balance, "addr_three funded");

    // ETH-format sends
    let eth_addr_two = harness.filecoin_to_eth(addr_two).await?;
    info!(%addr_two, eth_addr_two = ?eth_addr_two, "resolved ETH address for addr_two");
    let eth_addr_three = harness.filecoin_to_eth(addr_three).await?;
    info!(%addr_three, eth_addr_three = ?eth_addr_three, "resolved ETH address for addr_three");

    let msg_eth_local = harness
        .send_to_eth(Backend::Local, eth_addr_two, fil_amount.clone())
        .await?;
    info!(%msg_eth_local, "submitted local send to ETH-mapped addr_two");

    let msg_eth_remote = harness
        .send_to_eth(Backend::Remote, eth_addr_three, fil_amount.clone())
        .await?;
    info!(%msg_eth_remote, "submitted remote send to ETH-mapped addr_three");

    let after_eth_two = wait_balance_above(
        &harness,
        addr_two,
        addr_two_balance,
        "wait for addr_two balance to increase post-ETH-send",
    )
    .await?;
    info!(%addr_two, balance = %after_eth_two, "addr_two received ETH-routed send");

    let after_eth_three = wait_balance_above(
        &harness,
        addr_three,
        addr_three_balance,
        "wait for addr_three balance to increase post-ETH-send",
    )
    .await?;
    info!(%addr_three, balance = %after_eth_three, "addr_three received ETH-routed send");

    // Final delete-and-list smoke test
    smoke_test_delete(&mut harness, Backend::Local).await?;
    smoke_test_delete(&mut harness, Backend::Remote).await?;

    // TODO: Uncomment this check once the send command is fixed.
    // The recipient's on-chain balance after the first send should equal the
    // amount sent. `TokenAmount` is already unitless atto-FIL, so we compare
    // directly without the bash `cut -d ' ' -f 1` dance.
    //
    // if addr_two_balance != fil_amount {
    //     bail!(
    //         "FIL amount should match: addr_two balance {addr_two_balance:?}, sent {fil_amount:?}"
    //     );
    // }

    info!("basic wallet scenario completed successfully");
    Ok(())
}

async fn first_address(harness: &WalletHarness, backend: Backend) -> anyhow::Result<Address> {
    let addrs = harness.list(backend).await?;
    addrs
        .into_iter()
        .next_back()
        .context("wallet list is empty")
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

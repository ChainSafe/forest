// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Delegated wallet integration test
use anyhow::{Context as _, bail};
use forest::interop_tests_private::shim::{address::Address, crypto::SignatureType};
use tracing::info;

use crate::{Backend, WalletHarness, parse_amount, wait_balance_above, wait_balance_nonzero};

/// Run the delegated-wallet check.
///
/// Steps:
/// 1. Pick the preloaded `addr_one` from the local backend.
/// 2. Create `delegate_addr_one` (delegated) locally, export it, and import
///    it into the remote backend.
/// 3. Fund `delegate_addr_one` with 3 micro FIL from `addr_one`; poll for a
///    non-zero balance.
/// 4. Create `delegate_addr_two` locally and `delegate_addr_three` remotely.
///    Both backends now treat `delegate_addr_one` as the default.
/// 5. Send 500 atto FIL to `delegate_addr_two` and `delegate_addr_three`,
///    both signed locally (the local keystore holds copies of all three
///    delegated keys); poll both balances.
/// 6. Send another 500 atto FIL to `delegate_addr_three` from the remote
///    backend; poll for a further increase.
pub async fn run() -> anyhow::Result<()> {
    let mut harness = WalletHarness::from_env()?;

    let fil_amount = parse_amount("500 atto FIL")?;
    let delegate_fund_amount = parse_amount("3 micro FIL")?;

    let addr_one = first_address(&harness, Backend::Local)
        .await
        .context("expected a preloaded address in the local keystore")?;
    info!(%addr_one, "selected preloaded address");

    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Create the first delegated wallet, copy it to the remote backend
    info!("creating delegate_addr_one locally");
    let delegate_addr_one = harness
        .new_address(Backend::Local, SignatureType::Delegated)
        .await?;
    info!(%delegate_addr_one, "created delegate_addr_one");

    let exported = harness.export(Backend::Local, delegate_addr_one).await?;
    let imported_remote = harness.import(Backend::Remote, exported).await?;
    if imported_remote != delegate_addr_one {
        bail!(
            "remote import of delegate_addr_one returned {imported_remote}, expected {delegate_addr_one}"
        );
    }

    // Fund delegate_addr_one from addr_one
    harness.set_default(Backend::Local, addr_one).await?;
    let fund_msg = harness
        .send(
            Backend::Local,
            delegate_addr_one,
            delegate_fund_amount.clone(),
        )
        .await?;
    info!(%fund_msg, "submitted funding message to delegate_addr_one");

    let _funded_balance = wait_balance_nonzero(
        &harness,
        delegate_addr_one,
        "wait for delegate_addr_one balance",
    )
    .await?;
    info!(%delegate_addr_one, "delegate_addr_one funded");

    // Two more delegated wallets and reset defaults
    info!("creating delegate_addr_two locally");
    let delegate_addr_two = harness
        .new_address(Backend::Local, SignatureType::Delegated)
        .await?;
    info!(%delegate_addr_two, "created delegate_addr_two");
    harness
        .set_default(Backend::Local, delegate_addr_one)
        .await?;

    info!("creating delegate_addr_three remotely");
    let delegate_addr_three = harness
        .new_address(Backend::Remote, SignatureType::Delegated)
        .await?;
    info!(%delegate_addr_three, "created delegate_addr_three");
    harness
        .set_default(Backend::Remote, delegate_addr_one)
        .await?;

    // First round of sends from the local backend
    let msg_two = harness
        .send(Backend::Local, delegate_addr_two, fil_amount.clone())
        .await?;
    info!(%msg_two, "submitted local send to delegate_addr_two");

    let msg_three = harness
        .send(Backend::Local, delegate_addr_three, fil_amount.clone())
        .await?;
    info!(%msg_three, "submitted local send to delegate_addr_three");

    let _two_balance = wait_balance_nonzero(
        &harness,
        delegate_addr_two,
        "wait for delegate_addr_two balance",
    )
    .await?;

    let three_balance = wait_balance_nonzero(
        &harness,
        delegate_addr_three,
        "wait for delegate_addr_three balance",
    )
    .await?;

    // Second send to delegate_addr_three, from the remote backend
    let msg_three_again = harness
        .send(Backend::Remote, delegate_addr_three, fil_amount.clone())
        .await?;
    info!(%msg_three_again, "submitted remote send to delegate_addr_three");

    let after = wait_balance_above(
        &harness,
        delegate_addr_three,
        three_balance,
        "wait for delegate_addr_three balance to increase",
    )
    .await?;
    info!(%delegate_addr_three, balance = %after, "delegate_addr_three received remote send");

    info!("delegated wallet scenario completed successfully");
    Ok(())
}

async fn first_address(harness: &WalletHarness, backend: Backend) -> anyhow::Result<Address> {
    let addrs = harness.list(backend).await?;
    addrs
        .into_iter()
        .next_back()
        .context("wallet list is empty")
}

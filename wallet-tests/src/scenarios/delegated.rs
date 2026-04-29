// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Delegated wallet integration test
use anyhow::{Context as _, bail};
use forest::interop_tests_private::shim::crypto::SignatureType;
use tracing::info;

use crate::{Backend, WalletHarness, parse_amount, wait_balance_above, wait_balance_nonzero};

/// Run the delegated-wallet check.
///
/// Steps:
/// 1. Pick `preloaded` from the local backend.
/// 2. Create `deleg_shared` (delegated) locally, export it, and import it into
///    the remote backend.
/// 3. Fund `deleg_shared` with [`crate::DELEGATED_FUND_AMOUNT`] from
///    `preloaded`; poll for a non-zero balance.
/// 4. Create `deleg_local` locally and `deleg_remote` remotely. Both backends
///    now use `deleg_shared` as the default signing address.
/// 5. Send [`crate::TRANSFER_AMOUNT`] to `deleg_local` and `deleg_remote`, both
///    signed locally (the local keystore holds all three delegated keys); poll
///    both balances.
/// 6. Send another [`crate::TRANSFER_AMOUNT`] to `deleg_remote` from the remote
///    backend; poll for a further increase.
pub async fn run() -> anyhow::Result<()> {
    let mut harness = WalletHarness::from_env()?;

    let fil_amount = parse_amount(crate::TRANSFER_AMOUNT)?;
    let delegate_fund_amount = parse_amount(crate::DELEGATED_FUND_AMOUNT)?;

    let preloaded = harness
        .preloaded_address(Backend::Local)
        .await
        .context("expected a preloaded address in the local keystore")?;
    info!(%preloaded, "selected preloaded address");

    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // First delegated wallet: copy to the remote backend so both sides have it
    info!("creating deleg_shared locally");
    let deleg_shared = harness
        .new_address(Backend::Local, SignatureType::Delegated)
        .await?;
    info!(%deleg_shared, "created deleg_shared");

    let exported = harness.export(Backend::Local, deleg_shared).await?;
    let imported_remote = harness.import(Backend::Remote, exported).await?;
    if imported_remote != deleg_shared {
        bail!("remote import of deleg_shared returned {imported_remote}, expected {deleg_shared}");
    }

    harness.set_default(Backend::Local, preloaded).await?;
    let fund_msg = harness
        .send(Backend::Local, deleg_shared, delegate_fund_amount.clone())
        .await?;
    info!(%fund_msg, "submitted funding message to deleg_shared");

    let _ = wait_balance_nonzero(&harness, deleg_shared, "wait for deleg_shared balance").await?;
    info!(%deleg_shared, "deleg_shared funded");

    // Two more delegated wallets; defaults point back at deleg_shared
    info!("creating deleg_local locally");
    let deleg_local = harness
        .new_address(Backend::Local, SignatureType::Delegated)
        .await?;
    info!(%deleg_local, "created deleg_local");
    harness.set_default(Backend::Local, deleg_shared).await?;

    info!("creating deleg_remote remotely");
    let deleg_remote = harness
        .new_address(Backend::Remote, SignatureType::Delegated)
        .await?;
    info!(%deleg_remote, "created deleg_remote");
    harness.set_default(Backend::Remote, deleg_shared).await?;

    // First round of sends from the local backend
    let msg_to_local = harness
        .send(Backend::Local, deleg_local, fil_amount.clone())
        .await?;
    info!(%msg_to_local, "submitted local send to deleg_local");

    let msg_to_remote = harness
        .send(Backend::Local, deleg_remote, fil_amount.clone())
        .await?;
    info!(%msg_to_remote, "submitted local send to deleg_remote");

    let _ = wait_balance_nonzero(&harness, deleg_local, "wait for deleg_local balance").await?;

    let remote_balance =
        wait_balance_nonzero(&harness, deleg_remote, "wait for deleg_remote balance").await?;

    let msg_remote_again = harness
        .send(Backend::Remote, deleg_remote, fil_amount.clone())
        .await?;
    info!(%msg_remote_again, "submitted remote send to deleg_remote");

    let after_remote = wait_balance_above(
        &harness,
        deleg_remote,
        remote_balance,
        "wait for deleg_remote balance to increase",
    )
    .await?;
    info!(%deleg_remote, balance = %after_remote, "deleg_remote received remote send");

    info!("delegated wallet scenario completed successfully");
    Ok(())
}

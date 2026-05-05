// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Calibnet wallet integration tests. Each `#[tokio::test]` is `#[ignore]`
//! and assumes:
//! - `forest-wallet` is on `PATH`,
//! - a Forest daemon is running and synced to calibnet,
//! - [`PRELOADED_ADDRESS`] is funded and imported into both backends,
//! - `FULLNODE_API_INFO` is exported.

#[path = "common/calibnet_wallet_helpers.rs"]
mod helpers;

use anyhow::Context as _;
use helpers::*;
use serde_json::json;
use tokio::sync::OnceCell;

static FUNDED_DELEGATED: OnceCell<String> = OnceCell::const_new();

#[tokio::test]
#[ignore = "requires a running calibnet Forest daemon"]
async fn local_export_import_roundtrip() -> anyhow::Result<()> {
    let addr = wallet(&["new"])?;
    assert_export_import_roundtrip(&addr, false)
}

#[tokio::test]
#[ignore = "requires a running calibnet Forest daemon"]
async fn remote_export_import_roundtrip() -> anyhow::Result<()> {
    let addr = wallet_remote(&["new"])?;
    assert_export_import_roundtrip(&addr, true)
}

#[tokio::test]
#[ignore = "requires a running calibnet Forest daemon"]
async fn market_add_balance_message_on_chain() -> anyhow::Result<()> {
    const ATTO_FIL: &str = "23";
    let result = rpc_call(
        "Filecoin.MarketAddBalance",
        json!([PRELOADED_ADDRESS, PRELOADED_ADDRESS, ATTO_FIL]),
    )
    .await?;
    let msg_cid = cid_from_lotus_json_result(&result)?;
    poll_until_state_search_msg(&msg_cid).await
}

#[tokio::test]
#[ignore = "requires a running calibnet Forest daemon"]
async fn send_to_local_filecoin_address() -> anyhow::Result<()> {
    let target = wallet(&["new"])?;
    let _ = send_from(PRELOADED_ADDRESS, &target, FIL_AMT, true)?;
    let _ = poll_until_funded(&target, false).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires a running calibnet Forest daemon"]
async fn send_to_remote_filecoin_address() -> anyhow::Result<()> {
    let target = wallet_remote(&["new"])?;
    let _ = send_from(PRELOADED_ADDRESS, &target, FIL_AMT, true)?;
    let _ = poll_until_funded(&target, true).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires a running calibnet Forest daemon"]
async fn send_to_local_eth_equivalent() -> anyhow::Result<()> {
    send_to_eth_equivalent(false).await
}

#[tokio::test]
#[ignore = "requires a running calibnet Forest daemon"]
async fn send_to_remote_eth_equivalent() -> anyhow::Result<()> {
    send_to_eth_equivalent(true).await
}

#[tokio::test]
#[ignore = "requires a running calibnet Forest daemon"]
async fn local_wallet_delete() -> anyhow::Result<()> {
    assert_create_delete(false)
}

#[tokio::test]
#[ignore = "requires a running calibnet Forest daemon"]
async fn remote_wallet_delete() -> anyhow::Result<()> {
    assert_create_delete(true)
}

#[tokio::test]
#[ignore = "requires a running calibnet Forest daemon"]
async fn delegated_send_local_to_local() -> anyhow::Result<()> {
    let funded = funded_delegated_addr().await?;
    let target = wallet(&["new", "delegated"])?;
    let _ = send_from(funded, &target, FIL_AMT, true)?;
    let _ = poll_until_funded(&target, false).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires a running calibnet Forest daemon"]
async fn delegated_send_local_to_remote() -> anyhow::Result<()> {
    let funded = funded_delegated_addr().await?;
    let target = wallet_remote(&["new", "delegated"])?;
    let _ = send_from(funded, &target, FIL_AMT, true)?;
    let _ = poll_until_funded(&target, true).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires a running calibnet Forest daemon"]
async fn delegated_remote_send() -> anyhow::Result<()> {
    let funded = funded_delegated_addr().await?;
    let target = wallet_remote(&["new", "delegated"])?;
    let baseline = balance(&target, true)?;
    let _ = send_from(funded, &target, FIL_AMT, true)?;
    if baseline == FIL_ZERO {
        let _ = poll_until_funded(&target, true).await?;
    } else {
        let _ = poll_until_changed(&target, &baseline, true).await?;
    }
    Ok(())
}

async fn funded_delegated_addr() -> anyhow::Result<&'static str> {
    let addr = FUNDED_DELEGATED
        .get_or_try_init(|| async {
            let addr = wallet(&["new", "delegated"])?;
            let _ = send_from(PRELOADED_ADDRESS, &addr, DELEGATE_FUND_AMT, true)?;
            let _ = poll_until_funded(&addr, false).await?;
            let exported = export_to_temp_file(&addr, false)?;
            let path = exported
                .path()
                .to_str()
                .context("temp path is not valid UTF-8")?;
            let mirrored = wallet_remote(&["import", path])?;
            anyhow::ensure!(mirrored == addr, "mirror mismatch: {mirrored} != {addr}");
            Ok::<_, anyhow::Error>(addr)
        })
        .await?;
    Ok(addr.as_str())
}

async fn send_to_eth_equivalent(remote: bool) -> anyhow::Result<()> {
    let target = if remote {
        wallet_remote(&["new"])?
    } else {
        wallet(&["new"])?
    };
    let _ = send_from(PRELOADED_ADDRESS, &target, FIL_AMT, true)?;
    let baseline = poll_until_funded(&target, remote).await?;

    let eth = filecoin_to_eth(&target).await?;
    let _ = send_from(PRELOADED_ADDRESS, &eth, FIL_AMT, true)?;
    let _ = poll_until_changed(&target, &baseline, remote).await?;
    Ok(())
}

fn assert_export_import_roundtrip(address: &str, remote: bool) -> anyhow::Result<()> {
    let exported = export_to_temp_file(address, remote)?;
    let path = exported
        .path()
        .to_str()
        .context("temp path is not valid UTF-8")?;

    let delete_args = ["delete", address];
    let import_args = ["import", path];
    let imported = if remote {
        let _ = wallet_remote(&delete_args)?;
        wallet_remote(&import_args)?
    } else {
        let _ = wallet(&delete_args)?;
        wallet(&import_args)?
    };
    anyhow::ensure!(
        imported == address,
        "round-trip mismatch on {} backend: {imported} != {address}",
        if remote { "remote" } else { "local" }
    );
    Ok(())
}

fn assert_create_delete(remote: bool) -> anyhow::Result<()> {
    let new_args = ["new"];
    let addr = if remote {
        wallet_remote(&new_args)?
    } else {
        wallet(&new_args)?
    };

    let delete_args = ["delete", &addr];
    let listing = if remote {
        let _ = wallet_remote(&delete_args)?;
        wallet_remote(&["list"])?
    } else {
        let _ = wallet(&delete_args)?;
        wallet(&["list"])?
    };
    anyhow::ensure!(
        !listing.contains(&addr),
        "deleted wallet {addr} still appears in `list`:\n{listing}"
    );
    Ok(())
}

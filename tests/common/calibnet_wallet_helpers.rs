// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Helpers for the calibnet wallet integration tests in
//! [`tests/calibnet_wallet.rs`](../../calibnet_wallet.rs).

#![allow(dead_code)]

use std::io::Write as _;
use std::process::Command;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Context as _, bail};
use parking_lot::Mutex;
use serde_json::{Value, json};
use tempfile::NamedTempFile;

/// Calibnet address used as the funded source in every test. Imported into
/// both backends by the harness before this binary runs.
pub const PRELOADED_ADDRESS: &str = "t147upkwsnjhyabxuusawz3x42cselvdnp7j26kxy";

/// Default amount transferred in value-transfer assertions.
pub const FIL_AMT: &str = "500 atto FIL";
/// Sentinel `forest-wallet balance --exact-balance` returns for an unfunded address.
pub const FIL_ZERO: &str = "0 FIL";
/// Amount used to seed a freshly-created delegated wallet.
pub const DELEGATE_FUND_AMT: &str = "3 micro FIL";

/// Maximum number of times to retry a balance poll before timing out.
pub const POLL_RETRIES: usize = 20;
/// Delay between balance-poll attempts.
pub const POLL_DELAY: Duration = Duration::from_secs(30);

/// Retries for `Filecoin.StateSearchMsg` polling.
pub const SEARCH_MSG_RETRIES: usize = 30;
/// Delay between `StateSearchMsg` attempts.
pub const SEARCH_MSG_DELAY: Duration = Duration::from_secs(5);

/// Serializes `forest-wallet` invocations against the local keystore so
/// concurrent tests don't lose entries through last-writer-wins on the
/// keystore file. The daemon already serializes its own keystore handlers,
/// so remote invocations skip this lock.
static LOCAL_KEYSTORE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Run `forest-wallet <args>` against the local keystore and return trimmed
/// stdout on success.
pub fn wallet(args: &[&str]) -> anyhow::Result<String> {
    Ok(String::from_utf8(run_local_raw(args)?)?.trim().to_string())
}

/// Run `forest-wallet --remote-wallet <args>` and return trimmed stdout on
/// success.
pub fn wallet_remote(args: &[&str]) -> anyhow::Result<String> {
    let mut full = Vec::with_capacity(args.len() + 1);
    full.push("--remote-wallet");
    full.extend_from_slice(args);
    Ok(String::from_utf8(run_raw("forest-wallet", &full)?)?
        .trim()
        .to_string())
}

fn run_local_raw(args: &[&str]) -> anyhow::Result<Vec<u8>> {
    let _guard = LOCAL_KEYSTORE_LOCK.lock();
    run_raw("forest-wallet", args)
}

fn run_raw(bin: &str, args: &[&str]) -> anyhow::Result<Vec<u8>> {
    let output = Command::new(bin)
        .args(args)
        .output()
        .with_context(|| format!("failed to spawn `{bin}`"))?;
    if !output.status.success() {
        bail!(
            "`{bin} {}` failed (status={}): {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(output.stdout)
}

/// Export `address` from the chosen backend into a temp file ready to feed
/// back to `forest-wallet import`.
pub fn export_to_temp_file(address: &str, remote: bool) -> anyhow::Result<NamedTempFile> {
    let raw = if remote {
        run_raw("forest-wallet", &["--remote-wallet", "export", address])?
    } else {
        run_local_raw(&["export", address])?
    };
    let mut file = NamedTempFile::new_in(std::env::temp_dir())
        .context("failed to create temp file for wallet export")?;
    file.write_all(&raw)?;
    file.flush()?;
    Ok(file)
}

/// Run `forest-wallet [--remote-wallet] balance <addr> --exact-balance`.
pub fn balance(address: &str, remote: bool) -> anyhow::Result<String> {
    let args = ["balance", address, "--exact-balance"];
    if remote {
        wallet_remote(&args)
    } else {
        wallet(&args)
    }
}

/// Run `forest-wallet [--remote-wallet] send --from <from> <to> <amount>`.
/// Always passing `--from` keeps tests independent of the shared
/// `set-default` slot.
pub fn send_from(from: &str, to: &str, amount: &str, remote: bool) -> anyhow::Result<String> {
    let args = ["send", "--from", from, to, amount];
    if remote {
        wallet_remote(&args)
    } else {
        wallet(&args)
    }
}

/// Poll `check` up to [`POLL_RETRIES`] times with [`POLL_DELAY`] between
/// attempts; return the satisfying value or a labelled timeout error.
pub async fn poll<F>(what: &str, mut check: F) -> anyhow::Result<String>
where
    F: FnMut() -> anyhow::Result<Option<String>>,
{
    for i in 1..=POLL_RETRIES {
        eprintln!("Polling {what} {i}/{POLL_RETRIES}");
        tokio::time::sleep(POLL_DELAY).await;
        if let Some(value) = check()? {
            return Ok(value);
        }
    }
    bail!("Timed out waiting for {what} after {POLL_RETRIES} retries")
}

/// Poll until the balance reported for `address` is no longer [`FIL_ZERO`].
pub async fn poll_until_funded(address: &str, remote: bool) -> anyhow::Result<String> {
    let label = format!(
        "{} balance for {address}",
        if remote { "remote" } else { "local" }
    );
    poll(&label, || {
        let bal = balance(address, remote)?;
        Ok((bal != FIL_ZERO).then_some(bal))
    })
    .await
}

/// Poll until the balance reported for `address` differs from `baseline`.
pub async fn poll_until_changed(
    address: &str,
    baseline: &str,
    remote: bool,
) -> anyhow::Result<String> {
    let label = format!(
        "{} balance change for {address}",
        if remote { "remote" } else { "local" }
    );
    let baseline = baseline.to_string();
    poll(&label, || {
        let bal = balance(address, remote)?;
        Ok((bal != baseline).then_some(bal))
    })
    .await
}

/// Parse `FULLNODE_API_INFO` (`<token>:/ip4/<host>/tcp/<port>/http`) into
/// `(token, http_url)` where `http_url` is the v1 RPC endpoint.
pub fn parse_fullnode_api_info() -> anyhow::Result<(String, String)> {
    let raw = std::env::var("FULLNODE_API_INFO").context("FULLNODE_API_INFO env var not set")?;
    let (token, multiaddr) = raw
        .split_once(':')
        .context("FULLNODE_API_INFO must be `<token>:<multiaddr>`")?;
    let parts: Vec<&str> = multiaddr.split('/').collect();
    let host = parts
        .get(2)
        .filter(|s| !s.is_empty())
        .with_context(|| format!("missing host in multiaddr `{multiaddr}`"))?;
    let port = parts
        .get(4)
        .filter(|s| !s.is_empty())
        .with_context(|| format!("missing port in multiaddr `{multiaddr}`"))?;
    Ok((token.to_string(), format!("http://{host}:{port}/rpc/v1")))
}

/// POST a JSON-RPC v1 request to the daemon and return the `result` field.
pub async fn rpc_call(method: &str, params: Value) -> anyhow::Result<Value> {
    let (token, url) = parse_fullnode_api_info()?;
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let resp: Value = reqwest::Client::new()
        .post(&url)
        .bearer_auth(&token)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("POST {url} for {method}"))?
        .error_for_status()
        .with_context(|| format!("HTTP error from {method}"))?
        .json()
        .await
        .with_context(|| format!("decoding JSON-RPC response for {method}"))?;
    if let Some(err) = resp.get("error").filter(|e| !e.is_null()) {
        bail!("RPC error from {method}: {err}");
    }
    resp.get("result")
        .cloned()
        .with_context(|| format!("missing `result` in response for {method}"))
}

/// Same as [`rpc_call`], but maps missing or null `result` to `Ok(None)`.
pub async fn rpc_call_opt(method: &str, params: Value) -> anyhow::Result<Option<Value>> {
    let (token, url) = parse_fullnode_api_info()?;
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let resp: Value = reqwest::Client::new()
        .post(&url)
        .bearer_auth(&token)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("POST {url} for {method}"))?
        .error_for_status()
        .with_context(|| format!("HTTP error from {method}"))?
        .json()
        .await
        .with_context(|| format!("decoding JSON-RPC response for {method}"))?;
    if let Some(err) = resp.get("error").filter(|e| !e.is_null()) {
        bail!("RPC error from {method}: {err}");
    }
    match resp.get("result") {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => Ok(Some(v.clone())),
    }
}

/// Extracts a CID string from a JSON-RPC value (Lotus `{ "/": "bafy..." }` or plain string).
pub fn cid_from_lotus_json_result(result: &Value) -> anyhow::Result<String> {
    if let Some(s) = result.as_str() {
        return Ok(s.to_string());
    }
    result
        .get("/")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .with_context(|| format!("expected CID (lotus JSON or string), got {result}"))
}

/// Polls `Filecoin.StateSearchMsg` until the message is found or retries are exhausted (same cadence as
/// `scripts/tests/calibnet_wallet_check.sh`).
pub async fn poll_until_state_search_msg(msg_cid: &str) -> anyhow::Result<()> {
    for i in 1..=SEARCH_MSG_RETRIES {
        tokio::time::sleep(SEARCH_MSG_DELAY).await;
        eprintln!("StateSearchMsg polling {msg_cid} attempt {i}/{SEARCH_MSG_RETRIES}");
        let params = json!([[], { "/": msg_cid }, 800_i64, true]);
        if rpc_call_opt("Filecoin.StateSearchMsg", params)
            .await?
            .is_some()
        {
            return Ok(());
        }
    }
    bail!(
        "timed out waiting for message {msg_cid} via StateSearchMsg after {SEARCH_MSG_RETRIES} retries"
    )
}

/// Resolve the ETH equivalent of a Filecoin address via
/// `Filecoin.FilecoinAddressToEthAddress`.
pub async fn filecoin_to_eth(address: &str) -> anyhow::Result<String> {
    let result = rpc_call(
        "Filecoin.FilecoinAddressToEthAddress",
        json!([address, "pending"]),
    )
    .await?;
    result
        .as_str()
        .map(str::to_owned)
        .with_context(|| format!("expected string ETH address, got {result}"))
}

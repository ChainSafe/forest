// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::io::Write as _;
use std::process::Command;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Context as _, bail};
use parking_lot::Mutex;
use serde_json::{Value, json};
use tempfile::NamedTempFile;
use tokio::sync::OnceCell;

/// Funded preloaded address from env `FOREST_TEST_PRELOADED_ADDRESS` (`forest_wallet_init` in `scripts/tests/harness.sh`).
pub static FOREST_TEST_PRELOADED_ADDRESS: LazyLock<String> = LazyLock::new(|| {
    std::env::var("FOREST_TEST_PRELOADED_ADDRESS")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .expect("FOREST_TEST_PRELOADED_ADDRESS must be set")
});

/// Test amount to be transferred between accounts in wallet tests.
pub const FIL_AMT: &str = "500 atto FIL";
/// Sentinel `forest-wallet balance --exact-balance` returns for an unfunded address.
pub const FIL_ZERO: &str = "0 FIL";
/// Amount to seed a freshly-created delegated wallet.
pub const DELEGATE_FUND_AMT: &str = "3 micro FIL";

pub const POLL_RETRIES: usize = 20;
pub const POLL_DELAY: Duration = Duration::from_secs(30);

pub const SEARCH_MSG_RETRIES: usize = 30;
pub const SEARCH_MSG_DELAY: Duration = Duration::from_secs(5);

/// Selects which `forest-wallet` keystore an operation targets.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Backend {
    Local,
    Remote,
}

impl Backend {
    fn extra_args(self) -> &'static [&'static str] {
        match self {
            Self::Local => &[],
            Self::Remote => &["--remote-wallet"],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
        }
    }
}

/// Serializes local keystore file access.
static LOCAL_KEYSTORE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Run `forest-wallet [--remote-wallet] <args>` and return trimmed stdout.
pub fn wallet(backend: Backend, args: &[&str]) -> anyhow::Result<String> {
    Ok(String::from_utf8(run_wallet_raw(backend, args)?)?
        .trim()
        .to_string())
}

/// Same as [`wallet`] but yields raw stdout bytes (used by `export`).
pub fn run_wallet_raw(backend: Backend, args: &[&str]) -> anyhow::Result<Vec<u8>> {
    let _guard = (backend == Backend::Local).then(|| LOCAL_KEYSTORE_LOCK.lock());

    let mut full = Vec::with_capacity(backend.extra_args().len() + args.len());
    full.extend_from_slice(backend.extra_args());
    full.extend_from_slice(args);

    let output = Command::new("forest-wallet")
        .args(&full)
        .output()
        .context("failed to spawn `forest-wallet`")?;
    if !output.status.success() {
        bail!(
            "`forest-wallet {}` failed (status={}): {}",
            full.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(output.stdout)
}

/// Export `address` from the chosen backend into a temp file ready to feed
/// back to `forest-wallet import`.
pub fn export_to_temp_file(address: &str, backend: Backend) -> anyhow::Result<NamedTempFile> {
    let raw = run_wallet_raw(backend, &["export", address])?;
    let mut file = NamedTempFile::new_in(std::env::temp_dir())
        .context("failed to create temp file for wallet export")?;
    file.write_all(&raw)?;
    file.flush()?;
    Ok(file)
}

pub fn balance(address: &str, backend: Backend) -> anyhow::Result<String> {
    wallet(backend, &["balance", address, "--exact-balance"])
}

/// Send with `--from`. `backend` chooses the signing keystore
/// (local file vs `--remote-wallet`).
///
/// Retries on the transient `gas price is lower than min gas price` mpool
/// error: the local CLI path estimates gas, then submits via `MpoolPush`,
/// so a concurrent push that bumps the mempool's fee floor between
/// estimate and push rejects an otherwise-valid message. Retry re-runs
/// fee estimation so gas fields match whatever minimum gas price applies
/// at the next submission.
pub fn send_from(from: &str, to: &str, amount: &str, backend: Backend) -> anyhow::Result<String> {
    let args = ["send", "--from", from, to, amount];
    let mut attempt = 1;
    loop {
        match wallet(backend, &args) {
            Ok(out) => return Ok(out),
            Err(e) if attempt < SEND_RETRIES && is_min_gas_price_error(&e) => {
                eprintln!(
                    "send {from} -> {to} hit min-gas-price floor on attempt {attempt}/{SEND_RETRIES}, retrying"
                );
                std::thread::sleep(SEND_RETRY_DELAY);
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Max attempts for [`send_from`].
const SEND_RETRIES: usize = 3;
/// Delay between [`send_from`] retries; one block-time at calibnet cadence
/// is enough for the daemon's gas-price snapshot to refresh.
const SEND_RETRY_DELAY: Duration = Duration::from_secs(15);

fn is_min_gas_price_error(err: &anyhow::Error) -> bool {
    err.chain().any(|e| {
        e.to_string()
            .contains("gas price is lower than min gas price")
    })
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
pub async fn poll_until_funded(address: &str, backend: Backend) -> anyhow::Result<String> {
    let label = format!("{} balance for {address}", backend.label());
    poll(&label, || {
        let bal = balance(address, backend)?;
        Ok((bal != FIL_ZERO).then_some(bal))
    })
    .await
}

/// Poll until the balance reported for `address` differs from `baseline`.
pub async fn poll_until_changed(
    address: &str,
    baseline: &str,
    backend: Backend,
) -> anyhow::Result<String> {
    let label = format!("{} balance change for {address}", backend.label());
    let baseline = baseline.to_string();
    poll(&label, || {
        let bal = balance(address, backend)?;
        Ok((bal != baseline).then_some(bal))
    })
    .await
}

static FUNDED_DELEGATED: OnceCell<String> = OnceCell::const_new();

/// Delegated signer: create once on local, fund locally, mirror to remote
/// for tests that query or sign.
pub async fn funded_delegated_addr() -> &'static str {
    let addr = FUNDED_DELEGATED
        .get_or_try_init(|| async {
            let addr = wallet(Backend::Local, &["new", "delegated"]).unwrap();
            let fund_msg = send_from(
                &FOREST_TEST_PRELOADED_ADDRESS,
                &addr,
                DELEGATE_FUND_AMT,
                Backend::Local,
            )
            .unwrap();
            eprintln!("delegated funding send to {addr} msg: {fund_msg}");
            let funded = poll_until_funded(&addr, Backend::Local).await.unwrap();
            eprintln!("delegated wallet {addr} funded balance: {funded}");

            let exported = export_to_temp_file(&addr, Backend::Local).unwrap();
            let path = exported
                .path()
                .to_str()
                .expect("temp path is not valid UTF-8");
            let mirrored = wallet(Backend::Remote, &["import", path]).unwrap();
            assert_eq!(mirrored, addr, "mirror mismatch: {mirrored} != {addr}");
            Ok::<_, anyhow::Error>(addr)
        })
        .await
        .unwrap();
    addr.as_str()
}

static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .expect("failed to build reqwest client")
});

/// Cached `(token, http_url)` parsed once from `FULLNODE_API_INFO`.
static API: LazyLock<anyhow::Result<(String, String)>> = LazyLock::new(|| {
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
});

fn api() -> anyhow::Result<&'static (String, String)> {
    API.as_ref()
        .map_err(|e| anyhow::anyhow!("FULLNODE_API_INFO unavailable: {e}"))
}

/// POST a JSON-RPC v1 request and return the `result` field, or `None` if
/// the server responded without one.
pub async fn rpc_call_opt(method: &str, params: Value) -> anyhow::Result<Option<Value>> {
    let (token, url) = api()?;
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let resp: Value = HTTP
        .post(url)
        .bearer_auth(token)
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

/// Like [`rpc_call_opt`] but treats a missing `result` as an error.
pub async fn rpc_call(method: &str, params: Value) -> anyhow::Result<Value> {
    rpc_call_opt(method, params)
        .await?
        .with_context(|| format!("missing `result` in response for {method}"))
}

/// Extract a CID string from either a Lotus `{ "/": "bafy..." }` map or a
/// plain string.
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

/// Poll `Filecoin.StateSearchMsg` until the message is mined or retries exhaust.
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

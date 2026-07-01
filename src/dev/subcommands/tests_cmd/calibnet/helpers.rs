// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::future::Future;
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

/// Maximum time to wait for a polled condition before failing the test.
pub const POLL_TIMEOUT: Duration = Duration::from_secs(600);
/// Delay between poll attempts.
pub const POLL_WAIT_TIME: Duration = Duration::from_secs(1);

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
    let args = [
        "send",
        "--from",
        from,
        "--wait-confidence",
        "1",
        "--wait-timeout",
        "1m",
        to,
        amount,
    ];
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

/// Poll until `try_check` returns `Some` or [`POLL_TIMEOUT`] elapses, sleeping
/// [`POLL_WAIT_TIME`] between attempts.
async fn poll<F, Fut, T>(label: &str, mut try_check: F) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<Option<T>>>,
{
    let started = tokio::time::Instant::now();
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        eprintln!("Polling {label} attempt {attempt}");
        if let Some(value) = try_check().await? {
            return Ok(value);
        }
        if started.elapsed() >= POLL_TIMEOUT {
            bail!("Timed out waiting for {label} after {POLL_TIMEOUT:?}");
        }
        let remaining = POLL_TIMEOUT.saturating_sub(started.elapsed());
        tokio::time::sleep(POLL_WAIT_TIME.min(remaining)).await;
    }
}

/// Poll until the balance reported for `address` differs from `baseline`.
pub async fn poll_until_changed(
    address: &str,
    baseline: &str,
    backend: Backend,
) -> anyhow::Result<String> {
    let label = format!("{} balance change for {address}", backend.label());
    let baseline = baseline.to_string();
    poll(&label, || async {
        let bal = balance(address, backend)?;
        Ok((bal != baseline).then_some(bal))
    })
    .await
}

/// Poll until the balance reported for `address` is no longer [`FIL_ZERO`].
pub async fn poll_until_funded(address: &str, backend: Backend) -> anyhow::Result<String> {
    poll_until_changed(address, FIL_ZERO, backend).await
}

/// Delegated signer: create once on local, fund locally, mirror to remote
/// for tests that query or sign.
pub async fn funded_delegated_addr() -> &'static str {
    static FUNDED_DELEGATED: OnceCell<String> = OnceCell::const_new();

    FUNDED_DELEGATED
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
            for backend in [Backend::Local, Backend::Remote] {
                let funded = poll_until_funded(&addr, backend).await.unwrap();
                eprintln!(
                    "delegated wallet {addr} funded balance: {funded} ({})",
                    backend.label()
                );
            }

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
        .unwrap()
        .as_str()
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

/// Poll `Filecoin.StateSearchMsg` until the message is mined or [`POLL_TIMEOUT`] elapses.
pub async fn poll_until_state_search_msg(msg_cid: &str) -> anyhow::Result<()> {
    let label = format!("StateSearchMsg for {msg_cid}");
    poll(&label, || async {
        let params = json!([[], { "/": msg_cid }, 800_i64, true]);
        Ok((rpc_call_opt("Filecoin.StateSearchMsg", params)
            .await?
            .is_some())
        .then_some(()))
    })
    .await
}

/// Run `forest-cli <args>` and return trimmed stdout.
pub fn forest_cli(args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new("forest-cli")
        .args(args)
        .output()
        .context("failed to spawn `forest-cli`")?;
    if !output.status.success() {
        bail!(
            "`forest-cli {}` failed (status={}): {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// Next nonce for an address
pub fn mpool_nonce(address: &str) -> anyhow::Result<u64> {
    let out = forest_cli(&["mpool", "nonce", address])?;
    out.parse::<u64>()
        .with_context(|| format!("invalid mpool nonce output: {out}"))
}

/// Pending message nonces for `address` via `Filecoin.MpoolPending`.
pub async fn pending_nonces_for(address: &str) -> anyhow::Result<Vec<u64>> {
    let result = rpc_call("Filecoin.MpoolPending", json!([null])).await?;
    let entries = result
        .as_array()
        .with_context(|| format!("expected MpoolPending array, got {result}"))?;
    Ok(entries
        .iter()
        .filter_map(|entry| {
            let msg = entry.get("Message")?;
            (msg.get("From")?.as_str()? == address).then_some(msg.get("Nonce")?.as_u64()?)
        })
        .collect())
}

/// Poll until `address` has a pending message at `nonce`.
pub async fn poll_until_pending_nonce(address: &str, nonce: u64) -> anyhow::Result<()> {
    let label = format!("pending nonce {nonce} for {address}");
    let address = address.to_string();
    poll(&label, || async {
        let nonces = pending_nonces_for(&address).await?;
        Ok(nonces.contains(&nonce).then_some(()))
    })
    .await
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

pub fn block_on<F: Future + Send + Sync + 'static>(future: F) -> F::Output
where
    F::Output: Send + Sync + 'static,
{
    std::thread::spawn(|| {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(future)
    })
    .join()
    .unwrap()
}

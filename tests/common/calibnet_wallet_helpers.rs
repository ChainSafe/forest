// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Shared helpers for calibnet integration tests (`wallet`, `mpool_tools`).

use std::process::Command;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Context as _, bail};
use parking_lot::Mutex;
use serde_json::{Value, json};

/// Funded preloaded address from env `FOREST_TEST_PRELOADED_ADDRESS` (`forest_wallet_init` in `scripts/tests/harness.sh`).
pub static FOREST_TEST_PRELOADED_ADDRESS: LazyLock<String> = LazyLock::new(|| {
    std::env::var("FOREST_TEST_PRELOADED_ADDRESS")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .expect("FOREST_TEST_PRELOADED_ADDRESS must be set")
});

/// Sentinel `forest-wallet balance --exact-balance` returns for an unfunded address.
pub const FIL_ZERO: &str = "0 FIL";
/// Maximum time to wait for a polled condition before failing the test.
pub const POLL_TIMEOUT: Duration = Duration::from_secs(600);
/// Delay between poll attempts.
pub const POLL_WAIT_TIME: Duration = Duration::from_secs(1);
/// Max attempts for [`Backend::send`].
const SEND_RETRIES: usize = 3;
/// Delay between [`Backend::send`] retries; one block-time at calibnet cadence
/// is enough for the daemon's gas-price snapshot to refresh.
const SEND_RETRY_DELAY: Duration = Duration::from_secs(15);

/// Run a `PATH` binary, returning raw stdout
pub fn run_command(program: &str, args: &[&str]) -> anyhow::Result<Vec<u8>> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to spawn `{program}`"))?;
    if !output.status.success() {
        bail!(
            "`{program} {}` failed (status={}): {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(output.stdout)
}

/// Selects which `forest-wallet` keystore an operation targets.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Backend {
    Local,
    Remote,
}

/// Serializes local keystore file access.
static LOCAL_KEYSTORE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

impl Backend {
    fn extra_args(self) -> &'static [&'static str] {
        match self {
            Self::Local => &[],
            Self::Remote => &["--remote-wallet"],
        }
    }

    /// Run `forest-wallet [--remote-wallet] <args>`, returning raw stdout.
    pub fn run_raw(self, args: &[&str]) -> anyhow::Result<Vec<u8>> {
        let _guard = (self == Self::Local).then(|| LOCAL_KEYSTORE_LOCK.lock());

        let mut full = Vec::with_capacity(self.extra_args().len() + args.len());
        full.extend_from_slice(self.extra_args());
        full.extend_from_slice(args);

        run_command("forest-wallet", &full)
    }

    /// Run `forest-wallet [--remote-wallet] <args>` and return trimmed stdout.
    pub fn run(self, args: &[&str]) -> anyhow::Result<String> {
        Ok(String::from_utf8(self.run_raw(args)?)?.trim().to_string())
    }

    /// Send `amount` from `from` to `to`, signing with this keystore.
    ///
    /// Retries on the transient `gas price is lower than min gas price` mpool
    /// error: the local CLI path estimates gas, then submits via `MpoolPush`,
    /// so a concurrent push that bumps the mempool's fee floor between
    /// estimate and push rejects an otherwise-valid message. Retry re-runs
    /// fee estimation so gas fields match whatever minimum gas price applies
    /// at the next submission.
    pub fn send(self, from: &str, to: &str, amount: &str) -> anyhow::Result<String> {
        let args = ["send", "--from", from, to, amount];
        let mut attempt = 1;
        loop {
            match self.run(&args) {
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
}

fn is_min_gas_price_error(err: &anyhow::Error) -> bool {
    err.chain().any(|e| {
        e.to_string()
            .contains("gas price is lower than min gas price")
    })
}

/// Poll until `try_check` returns `Some` or [`POLL_TIMEOUT`] elapses, sleeping
/// [`POLL_WAIT_TIME`] between attempts.
pub async fn poll<F, Fut, T>(label: &str, mut try_check: F) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<Option<T>>>,
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

static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(POLL_TIMEOUT)
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

/// Waits for a sent message to be included on-chain and confirms it executed successfully.
pub async fn wait_for_msg(msg_cid: &str) -> anyhow::Result<()> {
    let params = json!([{ "/": msg_cid }, 0, -1_i64, true]);
    let result = rpc_call("Filecoin.StateWaitMsg", params).await?;
    let exit_code = result
        .get("Receipt")
        .and_then(|r| r.get("ExitCode"))
        .and_then(Value::as_i64)
        .with_context(|| format!("StateWaitMsg result missing Receipt.ExitCode: {result}"))?;
    if exit_code != 0 {
        bail!("message {msg_cid} landed on-chain but failed with exit code {exit_code}");
    }
    Ok(())
}

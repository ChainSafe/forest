// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::future::Future;
use std::io::Write as _;
use std::process::Command;
use std::str::FromStr as _;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Context as _, bail, ensure};
use cid::Cid;
use jsonrpsee::core::ClientError;
use libtest_mimic::{Failed, Trial};
use serde_json::{Value, json};
use tempfile::NamedTempFile;
use tokio::sync::OnceCell;

use crate::rpc::eth::types::EthAddress;
use crate::rpc::prelude::*;
use crate::rpc::types::{ApiTipsetKey, MessageLookup};
use crate::rpc::{Client, humanize_rpc_error};
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::state_tree::ActorState;
use crate::state_manager::FAILED_TO_LOAD_MESSAGE;
use crate::utils::encoding::{hex, keccak_256};

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
pub const DELEGATE_FUND_AMT: &str = "30 micro FIL";
/// Funds [`sender`]: pays for an on-chain submit and lets priced estimates afford a nested call.
const SENDER_FUND_AMT: &str = "10 FIL";

/// Maximum time to wait for a polled condition before failing the test.
pub const POLL_TIMEOUT: Duration = Duration::from_secs(600);
/// Delay between poll attempts.
pub const POLL_WAIT_TIME: Duration = Duration::from_secs(1);
/// Epochs a message search looks back over before giving up.
const MESSAGE_LOOKBACK: ChainEpoch = 800;

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

/// Run `forest-wallet [--remote-wallet] <args>` and return trimmed stdout.
pub fn wallet(backend: Backend, args: &[&str]) -> anyhow::Result<String> {
    Ok(String::from_utf8(run_wallet_raw(backend, args)?)?
        .trim()
        .to_string())
}

/// Same as [`wallet`] but yields raw stdout bytes.
pub fn run_wallet_raw(backend: Backend, args: &[&str]) -> anyhow::Result<Vec<u8>> {
    let mut full = Vec::with_capacity(backend.extra_args().len() + args.len());
    full.extend_from_slice(backend.extra_args());
    full.extend_from_slice(args);
    run("forest-wallet", &full)
}

/// Runs `program args...` and returns raw stdout, with stderr surfaced on failure.
fn run(program: &str, args: &[&str]) -> anyhow::Result<Vec<u8>> {
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

/// [`run`] with stdout as trimmed UTF-8.
fn run_str(program: &str, args: &[&str]) -> anyhow::Result<String> {
    Ok(String::from_utf8(run(program, args)?)?.trim().to_owned())
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
pub fn send_from(from: &str, to: &str, amount: &str, backend: Backend) -> anyhow::Result<String> {
    wallet_send(backend, from, to, amount, &[], true)
}

pub fn send_from_no_wait(
    from: &str,
    to: &str,
    amount: &str,
    backend: Backend,
) -> anyhow::Result<String> {
    wallet_send(backend, from, to, amount, &[], false)
}

/// `forest-wallet send` with optional extra flags. When `wait` is set, uses
/// `--wait-confidence 0 --wait-timeout 10m`.
pub fn wallet_send(
    backend: Backend,
    from: &str,
    to: &str,
    amount: &str,
    extra: &[&str],
    wait: bool,
) -> anyhow::Result<String> {
    let mut args = vec!["send", to, amount, "--from", from];
    args.extend_from_slice(extra);
    if wait {
        args.extend(["--wait-confidence", "0", "--wait-timeout", "10m"]);
    }
    wallet(backend, &args)
}

/// Parses the CID from `forest-wallet send` and returns its execution, which must have succeeded.
/// `send` with `--wait-confidence` already waited for inclusion; this checks the exit code.
pub async fn assert_send_ok(out: &str) -> anyhow::Result<MessageLookup> {
    let cid = Cid::from_str(
        out.lines()
            .last()
            .context("no cid from `forest-wallet send`")?
            .trim(),
    )?;
    let lookup = poll_until_message_executed(&forest_client()?, cid).await?;
    let exit = lookup.receipt.exit_code();
    ensure!(
        exit.is_success(),
        "message {cid} failed on chain with exit code {exit}"
    );
    Ok(lookup)
}

/// Submit an EVM contract call via `forest-wallet send --params-hex`.
/// Imports `from` from the Lotus keystore into Forest's remote wallet when needed.
pub async fn wallet_send_calldata(
    from: &str,
    to: &EthAddress,
    calldata: &[u8],
    gas_limit: u64,
) -> anyhow::Result<Cid> {
    if wallet(Backend::Remote, &["has", from])? != "true" {
        import_lotus_wallet_into_forest(from)?;
    }
    let to_hex = hex::encode_prefixed(to.0.as_bytes());
    let params = hex::encode(calldata);
    let gas = gas_limit.to_string();
    let out = wallet_send(
        Backend::Remote,
        from,
        &to_hex,
        "0",
        &["--params-hex", params.as_str(), "--gas-limit", gas.as_str()],
        true,
    )?;
    Ok(assert_send_ok(&out).await?.message)
}

/// Max attempts for [`rpc_call_with_retry`].
const RPC_RETRIES: usize = 3;
/// Delay between [`rpc_call_with_retry`] attempts; one block-time at calibnet
/// cadence is enough for the daemon's state snapshot to refresh.
const RPC_RETRY_DELAY: Duration = Duration::from_secs(15);

/// Poll until `try_check` returns `Some` or [`POLL_TIMEOUT`] elapses, sleeping
/// [`POLL_WAIT_TIME`] between attempts.
pub async fn poll<F, Fut, T>(label: &str, try_check: F) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<Option<T>>>,
{
    poll_with(label, POLL_TIMEOUT, POLL_WAIT_TIME, try_check).await
}

/// Polls until `try_check` returns `Some` or `budget` elapses, sleeping `pause` between attempts.
///
/// # Errors
/// Returns the first error of `try_check` at once, or a timeout once `budget` has passed.
pub async fn poll_with<F, Fut, T>(
    label: &str,
    budget: Duration,
    pause: Duration,
    mut try_check: F,
) -> anyhow::Result<T>
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
        if started.elapsed() >= budget {
            bail!("Timed out waiting for {label} after {budget:?}");
        }
        let remaining = budget.saturating_sub(started.elapsed());
        tokio::time::sleep(pause.min(remaining)).await;
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

pub async fn get_actor(client: &Client, addr: Address) -> anyhow::Result<Option<ActorState>> {
    match client
        .call(StateGetActor::request((addr, ApiTipsetKey(None)))?)
        .await
    {
        Ok(actor) => Ok(actor),
        Err(e) if is_unknown_actor_error(&e) => Ok(None),
        Err(e) => Err(anyhow::anyhow!("{e:#}")),
    }
}

/// Whether a node answered that it does not have the actor (yet).
pub fn is_unknown_actor_error(e: &impl std::fmt::Display) -> bool {
    let text = format!("{e:#}");
    ["actor not found", "resolution lookup failed"]
        .iter()
        .any(|s| text.contains(s))
}

/// Poll until `node`'s state has an actor at `addr` (the Lotus node trails Forest by a block on the
/// forest-produced devnet, so a Forest-funded sender must be awaited before any `lotus --from`).
pub async fn poll_until_actor_on(
    node: &str,
    addr: Address,
    make_client: fn() -> anyhow::Result<Client>,
) -> anyhow::Result<ActorState> {
    poll(&format!("{node} StateGetActor {addr}"), || async {
        get_actor(&make_client()?, addr).await
    })
    .await
}

/// Lotus `wallet new` string (`t4…`) plus parsed Filecoin and ETH forms.
pub struct Wallet {
    pub cli: String,
    pub f4: Address,
    pub eth: EthAddress,
}

/// A fresh Lotus delegated wallet with no balance.
pub async fn new_unfunded() -> anyhow::Result<Wallet> {
    let cli = lotus_exec(&["wallet", "new", "delegated"])?;
    let f4 = Address::from_str(&cli).context("parsing unfunded delegated address")?;
    let eth = EthAddress::from_filecoin_address(&f4)?;
    Ok(Wallet { cli, f4, eth })
}

/// A fresh Lotus delegated wallet funded with `amount` and known to both nodes.
pub async fn new_funded(amount: &str) -> anyhow::Result<Wallet> {
    let wallet = new_unfunded().await?;
    fund_on_chain(&wallet.cli, amount).await?;
    Ok(wallet)
}

/// Funds `cli_addr` (Lotus `t4…` form) from the harness wallet and waits until both nodes have the
/// actor, so a test can use it on either node or pin a block after it. Pass the Lotus string into
/// `forest-wallet`; it rejects Forest `Address::to_string()` (`f4…`) while `CurrentNetwork` stays
/// Mainnet.
pub async fn fund_on_chain(cli_addr: &str, amount: &str) -> anyhow::Result<Address> {
    let addr = Address::from_str(cli_addr).context("parsing funded delegated address")?;
    let msg = send_from(
        &FOREST_TEST_PRELOADED_ADDRESS,
        cli_addr,
        amount,
        Backend::Local,
    )?;
    eprintln!("funding {cli_addr} with {amount}, msg: {msg}");
    let balance = poll_until_funded(cli_addr, Backend::Local).await?;
    eprintln!("{cli_addr} funded on forest, balance: {balance}");
    poll_until_actor_on("lotus", addr, lotus_client).await?;
    Ok(addr)
}

/// The funded sender, created once per process and known to both nodes and both wallets: a
/// dedicated wallet, since the genesis/miner key races with the miner on nonce.
pub async fn sender() -> anyhow::Result<&'static Wallet> {
    static SENDER: OnceCell<Wallet> = OnceCell::const_new();
    SENDER
        .get_or_try_init(|| async {
            let wallet = new_funded(SENDER_FUND_AMT).await?;
            import_lotus_wallet_into_forest(&wallet.cli)?;
            Ok(wallet)
        })
        .await
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

/// Reads a variable that sourcing the devnet test harness sets.
pub fn harness_env(name: &str) -> anyhow::Result<String> {
    std::env::var(name).with_context(|| format!("{name} not set; source the devnet test harness"))
}

/// Typed client for the Lotus node on the docker devnet, whose read methods need no token.
pub fn lotus_client() -> anyhow::Result<crate::rpc::Client> {
    let port = harness_env("LOTUS_RPC_PORT")?;
    Ok(crate::rpc::Client::from_url(
        format!("http://127.0.0.1:{port}/").parse()?,
    ))
}

/// Typed client for the Forest node under test, from `FULLNODE_API_INFO`.
pub fn forest_client() -> anyhow::Result<crate::rpc::Client> {
    crate::rpc::Client::default_or_from_env(None)
}

pub fn docker(args: &[&str]) -> anyhow::Result<String> {
    run_str("docker", args).context("is the devnet up?")
}

/// Runs `lotus <args>` inside the `lotus` container on the docker devnet.
pub fn lotus_exec(args: &[&str]) -> anyhow::Result<String> {
    let mut full = vec!["exec", "lotus", "lotus"];
    full.extend_from_slice(args);
    docker(&full)
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

/// Like [`rpc_call_opt`] but treats a missing `result` as an error and retries.
pub async fn rpc_call_with_retry(method: &str, params: Value) -> anyhow::Result<Value> {
    let mut attempt = 1;
    loop {
        let result = rpc_call_opt(method, params.clone()).await.and_then(|opt| {
            opt.with_context(|| format!("missing `result` in response for {method}"))
        });
        match result {
            Ok(v) => return Ok(v),
            Err(e) if attempt < RPC_RETRIES => {
                eprintln!(
                    "error: {e:?} {method} failed on attempt {attempt}/{RPC_RETRIES}, retrying"
                );
                tokio::time::sleep(RPC_RETRY_DELAY).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
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
        let params = json!([[], { "/": msg_cid }, MESSAGE_LOOKBACK, true]);
        Ok((rpc_call_opt("Filecoin.StateSearchMsg", params)
            .await?
            .is_some())
        .then_some(()))
    })
    .await
}

/// The JSON-RPC error object behind `err`, if the node answered with one.
pub fn rpc_call_err(err: &anyhow::Error) -> Option<&jsonrpsee::types::ErrorObjectOwned> {
    match err.downcast_ref::<ClientError>() {
        Some(ClientError::Call(obj)) => Some(obj),
        _ => None,
    }
}

/// Forest and Lotus both refuse a wait for a message they have never seen, rather than waiting
/// for one to arrive.
fn is_unseen_message_error(e: &ClientError) -> bool {
    matches!(e, ClientError::Call(obj) if obj.message().contains(FAILED_TO_LOAD_MESSAGE))
}

/// Wait until `cid` has been executed on the chain `client` follows. `lotus send` returns as soon
/// as Lotus's own mpool accepts the message, so the node under test may not have it yet.
pub async fn poll_until_message_executed(
    client: &Client,
    cid: Cid,
) -> anyhow::Result<MessageLookup> {
    // A blocking attempt outlives the loop's own deadline check, so hand each one what is left.
    let deadline = tokio::time::Instant::now() + POLL_TIMEOUT;
    poll(&format!("StateWaitMsg for {cid}"), || async {
        let budget = deadline.saturating_duration_since(tokio::time::Instant::now());
        match client
            .call(StateWaitMsg::request((cid, 0, MESSAGE_LOOKBACK, true))?.with_timeout(budget))
            .await
        {
            Ok(lookup) => Ok(Some(lookup)),
            Err(e) if is_unseen_message_error(&e) => Ok(None),
            Err(e) => Err(humanize_rpc_error(e.into())),
        }
    })
    .await
}

pub fn forest_cli(args: &[&str]) -> anyhow::Result<String> {
    run_str("forest-cli", args)
}

/// Import `addr` from the Lotus keystore into Forest's remote wallet.
pub fn import_lotus_wallet_into_forest(addr: &str) -> anyhow::Result<()> {
    let hex = lotus_exec(&["wallet", "export", addr])?;
    let mut file = NamedTempFile::new_in(std::env::temp_dir())
        .context("failed to create temp file for lotus wallet export")?;
    file.write_all(hex.trim().as_bytes())?;
    file.flush()?;
    let path = file
        .path()
        .to_str()
        .context("temp path is not valid UTF-8")?;
    wallet(Backend::Remote, &["import", path])?;
    Ok(())
}

/// Deploy hex bytecode with `forest-cli evm deploy --hex`.
pub fn forest_evm_deploy_hex(from: &str, bytecode_hex: &str) -> anyhow::Result<String> {
    let mut hex_file =
        NamedTempFile::new_in(std::env::temp_dir()).context("staging the contract bytecode")?;
    hex_file.write_all(bytecode_hex.trim().as_bytes())?;
    hex_file.flush()?;
    let path = hex_file
        .path()
        .to_str()
        .context("temp path is not valid UTF-8")?;
    forest_cli(&["evm", "deploy", "--from", from, "--hex", path])
}

/// Invoke a contract with `forest-cli evm invoke`.
pub fn forest_evm_invoke(from: &str, to: &str, calldata_hex: &str) -> anyhow::Result<String> {
    forest_cli(&["evm", "invoke", "--from", from, to, calldata_hex])
}

/// Parse the `f4 Address:` line from `forest-cli evm deploy` output.
pub fn parse_f4_from_evm_deploy(out: &str) -> anyhow::Result<Address> {
    let f4 = out
        .lines()
        .find_map(|l| l.trim().strip_prefix("f4 Address: "))
        .with_context(|| format!("no `f4 Address:` in deploy output:\n{out}"))?;
    Address::from_str(f4.trim()).context("parsing the deployed f4 address")
}

/// Deploys `bytecode_hex` from [`sender`] and waits until both nodes have the contract.
pub async fn deploy_contract(name: &str, bytecode_hex: &str) -> anyhow::Result<EthAddress> {
    let deploy = forest_evm_deploy_hex(&sender().await?.cli, bytecode_hex)?;
    let f4 = parse_f4_from_evm_deploy(&deploy)?;
    eprintln!("deployed {name} at {f4}");
    poll_until_actor_on("forest", f4, forest_client).await?;
    poll_until_actor_on("lotus", f4, lotus_client).await?;
    EthAddress::from_filecoin_address(&f4)
}

/// The 4-byte Ethereum function selector: first 4 bytes of `keccak256(signature)`.
pub fn selector(signature: &str) -> Vec<u8> {
    keccak_256(signature.as_bytes())
        .get(..4)
        .expect("keccak256 is 32 bytes")
        .to_vec()
}

/// An address as a 32-byte ABI word: 12 zero bytes, then the 20 address bytes.
pub fn abi_address_word(addr: EthAddress) -> Vec<u8> {
    let mut word = vec![0u8; 12];
    word.extend_from_slice(addr.0.as_bytes());
    word
}

/// ABI calldata: the selector of `signature`, then the already encoded `args`.
pub fn calldata(signature: &str, args: &[u8]) -> Vec<u8> {
    let mut out = selector(signature);
    out.extend_from_slice(args);
    out
}

/// Next nonce for an address
pub fn mpool_nonce(address: &str) -> anyhow::Result<u64> {
    let out = forest_cli(&["mpool", "nonce", address])?;
    out.parse::<u64>()
        .with_context(|| format!("invalid mpool nonce output: {out}"))
}

/// Pending message nonces for `address` via `Filecoin.MpoolPending`.
pub async fn pending_nonces_for(address: &str) -> anyhow::Result<Vec<u64>> {
    let result = rpc_call_with_retry("Filecoin.MpoolPending", json!([null])).await?;
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
    let result = rpc_call_with_retry(
        "Filecoin.FilecoinAddressToEthAddress",
        json!([address, "pending"]),
    )
    .await?;
    result
        .as_str()
        .map(str::to_owned)
        .with_context(|| format!("expected string ETH address, got {result}"))
}

/// A trial that runs `body` and reports a failure with its full error chain.
pub fn trial(name: &'static str, body: fn() -> anyhow::Result<()>) -> Trial {
    Trial::test(name, move || {
        body().map_err(|e| Failed::from(format!("{e:?}")))
    })
}

pub fn block_on<F: Future + Send + 'static>(future: F) -> F::Output
where
    F::Output: Send + 'static,
{
    std::thread::spawn(|| {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(future)
    })
    .join()
    // Preserve the panic message instead of `unwrap`'s `Any { .. }`.
    .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
}

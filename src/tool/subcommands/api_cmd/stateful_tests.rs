// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::eth::EVMMethod;
use crate::message::SignedMessage;
use crate::networks::calibnet::ETH_CHAIN_ID;
use crate::rpc::eth::EthUint64;
use crate::rpc::eth::pubsub_trait::SubscriptionKind;
use crate::rpc::eth::types::*;
use crate::rpc::types::ApiTipsetKey;
use crate::rpc::{self, RpcMethod, prelude::*};
use crate::shim::{address::Address, message::Message};

use anyhow::{Context, ensure};
use cbor4ii::core::Value;
use cid::Cid;
use ethereum_types::H256;
use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use tokio_util::sync::CancellationToken;

use std::io::{self, Write};
use std::pin::Pin;
use std::sync::Arc;

type TestRunner = Arc<
    dyn Fn(Arc<rpc::Client>) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub struct TestTransaction {
    pub to: Address,
    pub from: Address,
    pub payload: Vec<u8>,
    pub topic: EthHash,
}

#[derive(Clone)]
pub struct RpcTestScenario {
    pub run: TestRunner,
    pub name: Option<&'static str>,
    pub should_fail_with: Option<&'static str>,
    pub used_methods: Vec<&'static str>,
    pub ignore: Option<&'static str>,
}

impl RpcTestScenario {
    /// Create a basic scenario from a simple async closure.
    pub fn basic<F, Fut>(run_fn: F) -> Self
    where
        F: Fn(Arc<rpc::Client>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let run = Arc::new(move |client: Arc<rpc::Client>| {
            Box::pin(run_fn(client)) as Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>
        });
        Self {
            run,
            name: Default::default(),
            should_fail_with: Default::default(),
            used_methods: Default::default(),
            ignore: None,
        }
    }

    fn name(mut self, name: &'static str) -> Self {
        self.name = Some(name);
        self
    }

    pub fn should_fail_with(mut self, msg: &'static str) -> Self {
        self.should_fail_with = Some(msg);
        self
    }

    fn using<const ARITY: usize, M>(mut self) -> Self
    where
        M: RpcMethod<ARITY>,
    {
        self.used_methods.push(M::NAME);
        if let Some(alias) = M::NAME_ALIAS {
            self.used_methods.push(alias);
        }
        self
    }

    fn _ignore(mut self, msg: &'static str) -> Self {
        self.ignore = Some(msg);
        self
    }
}

pub(super) async fn run_tests(
    tests: impl IntoIterator<Item = RpcTestScenario> + Clone,
    client: impl Into<Arc<rpc::Client>>,
    filter: String,
) -> anyhow::Result<()> {
    let client: Arc<rpc::Client> = client.into();
    let mut passed = 0;
    let mut failed = 0;
    let mut ignored = 0;
    let mut filtered = 0;

    println!("running {} tests", tests.clone().into_iter().count());

    for (i, test) in tests.into_iter().enumerate() {
        if !filter.is_empty() && !test.used_methods.iter().any(|m| m.starts_with(&filter)) {
            filtered += 1;
            continue;
        }
        if test.ignore.is_some() {
            ignored += 1;
            println!(
                "test {} ... ignored",
                if let Some(name) = test.name {
                    name.to_string()
                } else {
                    format!("#{i}")
                },
            );
            continue;
        }

        print!(
            "test {} ... ",
            if let Some(name) = test.name {
                name.to_string()
            } else {
                format!("#{i}")
            }
        );

        io::stdout().flush()?;

        let result = (test.run)(client.clone()).await;

        match result {
            Ok(_) => {
                if let Some(expected_msg) = test.should_fail_with {
                    println!("FAILED (expected failure containing '{expected_msg}')");
                    failed += 1;
                } else {
                    println!("ok");
                    passed += 1;
                }
            }
            Err(e) => {
                if let Some(expected_msg) = test.should_fail_with {
                    let err_str = format!("{e:#}");
                    if err_str
                        .to_lowercase()
                        .contains(&expected_msg.to_lowercase())
                    {
                        println!("ok");
                        passed += 1;
                    } else {
                        println!("FAILED ({e:#})");
                        failed += 1;
                    }
                } else {
                    println!("FAILED {e:#}");
                    failed += 1;
                }
            }
        }
    }
    let status = if failed == 0 { "ok" } else { "FAILED" };
    println!(
        "test result: {status}. {passed} passed; {failed} failed; {ignored} ignored; {filtered} filtered out"
    );
    ensure!(failed == 0, "{failed} test(s) failed");
    Ok(())
}

/// A client-side WebSocket stream to the node's JSON-RPC endpoint.
type EthSubStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Open a WebSocket to the node's JSON-RPC endpoint (`/rpc/v1`).
/// Returns the live stream.
async fn connect_ws(client: &rpc::Client) -> anyhow::Result<EthSubStream> {
    let mut url = client.base_url().clone();
    let ws_scheme = match url.scheme() {
        "http" => "ws",
        "https" => "wss",
        scheme => anyhow::bail!("unsupported RPC URL scheme: {scheme}"),
    };
    url.set_scheme(ws_scheme)
        .map_err(|_| anyhow::anyhow!("failed to set scheme"))?;
    url.set_path("/rpc/v1");
    let (ws_stream, _) = connect_async(url.as_str()).await?;
    Ok(ws_stream)
}

async fn wait_next_epoch(client: &rpc::Client) -> anyhow::Result<()> {
    let base = client.call(ChainHead::request(())?).await?.epoch();
    tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if client.call(ChainHead::request(())?).await?.epoch() > base {
                break Ok(());
            }
        }
    })
    .await
    .context("timeout waiting for the next epoch")?
}

/// Poll `MpoolPending` until `message_cid` is visible. Returns while the message
/// is still pending; it does not wait for on-chain inclusion.
async fn wait_in_mempool(client: &rpc::Client, message_cid: Cid) -> anyhow::Result<()> {
    let mut retries = 100;
    loop {
        let pending = client
            .call(MpoolPending::request((ApiTipsetKey(None),))?)
            .await?;
        if pending.0.iter().any(|msg| msg.cid() == message_cid) {
            break Ok(());
        }
        ensure!(retries != 0, "Message not found in mpool");
        retries -= 1;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Wait for `message_cid` to appear in the mempool and then be included on chain.
async fn wait_pending_message(client: &rpc::Client, message_cid: Cid) -> anyhow::Result<()> {
    let tipset = client.call(ChainHead::request(())?).await?;
    wait_in_mempool(client, message_cid).await?;
    client
        .call(
            StateWaitMsg::request((message_cid, 1, tipset.epoch(), true))?
                .with_timeout(Duration::from_secs(300)),
        )
        .await?;
    Ok(())
}

/// Poll `eth_getFilterChanges` until the hashes seen so far contain `want`.
///
/// The filter collects pending `txs` on a background task, so a poll right after a
/// tx appears in the mempool may run before the collector has processed it.
/// Each poll consumes the filter's buffer, so hashes are accumulated across
/// polls and the full set seen is returned.
async fn poll_pending_filter_until(
    client: &rpc::Client,
    filter_id: &FilterID,
    want: &EthHash,
) -> anyhow::Result<Vec<EthHash>> {
    let mut seen: Vec<EthHash> = Vec::new();
    let mut retries = 100;
    loop {
        let result = client
            .call(EthGetFilterChanges::request((filter_id.clone(),))?)
            .await?;
        let EthFilterResult::Hashes(hashes) = result else {
            anyhow::bail!("expected hashes, got {result:?}");
        };
        seen.extend(hashes);
        if seen.contains(want) {
            break Ok(seen);
        }
        ensure!(
            retries != 0,
            "filter did not return {want:?} in time; saw {seen:?}"
        );
        retries -= 1;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn invoke_contract(client: &rpc::Client, tx: &TestTransaction) -> anyhow::Result<Cid> {
    let encoded_params = cbor4ii::serde::to_vec(
        Vec::with_capacity(tx.payload.len()),
        &Value::Bytes(tx.payload.clone()),
    )
    .context("failed to encode params")?;
    let nonce = client.call(MpoolGetNonce::request((tx.from,))?).await?;
    let message = Message {
        to: tx.to,
        from: tx.from,
        sequence: nonce,
        method_num: EVMMethod::InvokeContract as u64,
        params: encoded_params.into(),
        ..Default::default()
    };
    let unsigned_msg = client
        .call(GasEstimateMessageGas::request((
            message,
            None,
            ApiTipsetKey(None),
        ))?)
        .await?;

    let eth_tx_args = crate::eth::EthEip1559TxArgsBuilder::default()
        .chain_id(ETH_CHAIN_ID)
        .unsigned_message(&unsigned_msg)?
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build EIP-1559 transaction: {}", e))?;
    let eth_tx = crate::eth::EthTx::from(eth_tx_args);
    let data = eth_tx.rlp_unsigned_message(ETH_CHAIN_ID)?;

    let sig = client.call(WalletSign::request((tx.from, data))?).await?;
    let smsg = SignedMessage::new_unchecked(unsigned_msg, sig);
    let cid = smsg.cid();

    client.call(MpoolPush::request((smsg,))?).await?;

    Ok(cid)
}

/// Open a WebSocket and start an `eth_subscribe` subscription of the given
/// `kind`, with optional `filter` params (used by `logs`). Returns the live
/// stream and the assigned subscription id.
async fn open_eth_subscription(
    client: &rpc::Client,
    kind: SubscriptionKind,
    filter: Option<serde_json::Value>,
) -> anyhow::Result<(EthSubStream, serde_json::Value)> {
    let mut ws_stream = connect_ws(client).await?;

    let mut params = vec![serde_json::to_value(kind)?];
    params.extend(filter);
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_subscribe",
        "params": params,
    });
    ws_stream
        .send(WsMessage::Text(request.to_string().into()))
        .await
        .context("failed to send eth_subscribe request")?;

    // The acknowledgement carries the subscription id in `result`.
    let subscription_id = loop {
        let msg = match tokio::time::timeout(Duration::from_secs(30), ws_stream.next()).await {
            Ok(Some(msg)) => msg,
            Ok(None) => anyhow::bail!("WebSocket stream closed before eth_subscribe ack"),
            Err(_) => anyhow::bail!("timeout waiting for eth_subscribe ack"),
        };
        match msg {
            Ok(WsMessage::Text(text)) => {
                let json: serde_json::Value = serde_json::from_str(&text)?;
                if let Some(error) = json.get("error") {
                    anyhow::bail!("eth_subscribe failed: {error}");
                }
                if let Some(result) = json.get("result") {
                    break result.clone();
                }
            }
            Err(..) | Ok(WsMessage::Close(..)) => {
                anyhow::bail!("WebSocket closed before eth_subscribe ack")
            }
            _ => {}
        }
    };

    Ok((ws_stream, subscription_id))
}

/// Wait for the next `eth_subscription` notification on `subscription_id` and
/// return its `result` payload.
async fn next_subscription_payload(
    ws_stream: &mut EthSubStream,
    subscription_id: &serde_json::Value,
    timeout: Duration,
) -> anyhow::Result<serde_json::Value> {
    loop {
        let msg = match tokio::time::timeout(timeout, ws_stream.next()).await {
            Ok(Some(msg)) => msg,
            Ok(None) => anyhow::bail!("WebSocket stream closed"),
            Err(_) => anyhow::bail!("timeout waiting for subscription notification"),
        };
        match msg {
            Ok(WsMessage::Text(text)) => {
                let json: serde_json::Value = serde_json::from_str(&text)?;
                if json.get("method").and_then(|m| m.as_str()) != Some("eth_subscription") {
                    continue;
                }
                let params = json
                    .get("params")
                    .context("subscription notification missing params")?;
                anyhow::ensure!(
                    params.get("subscription") == Some(subscription_id),
                    "subscription id mismatch in notification"
                );
                return params
                    .get("result")
                    .cloned()
                    .context("subscription notification missing result");
            }
            Err(..) | Ok(WsMessage::Close(..)) => anyhow::bail!("WebSocket closed unexpectedly"),
            _ => {}
        }
    }
}

/// Cancel the subscription and close the WebSocket. Best-effort: the node also
/// drops the subscription when the socket closes.
async fn close_eth_subscription(
    ws_stream: &mut EthSubStream,
    subscription_id: &serde_json::Value,
) -> anyhow::Result<()> {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "eth_unsubscribe",
        "params": [subscription_id],
    });
    ws_stream
        .send(WsMessage::Text(request.to_string().into()))
        .await
        .context("failed to send eth_unsubscribe request")?;
    ws_stream.close(None).await?;
    Ok(())
}

fn eth_subscribe_new_heads() -> RpcTestScenario {
    RpcTestScenario::basic(|client| async move {
        // Remember the chain head before subscribing so we can prove the head we
        // receive is real, advancing — not stale or random data.
        let start = client.call(EthBlockNumber::request(())?).await?;

        let (mut ws_stream, subscription_id) =
            open_eth_subscription(&client, SubscriptionKind::NewHeads, None).await?;

        // A new head is published once per tipset (~30s on calibnet).
        let payload =
            next_subscription_payload(&mut ws_stream, &subscription_id, Duration::from_secs(180))
                .await;

        let _ = close_eth_subscription(&mut ws_stream, &subscription_id).await;
        let payload = payload?;

        // `newHeads` must yield a block-header object
        anyhow::ensure!(
            payload.is_object(),
            "newHeads must yield a block-header object, got: {payload}"
        );
        let header: ApiHeaders = serde_json::from_value(payload)
            .context("newHeads payload is not a valid Eth block header")?;

        // Identity: the header's number must be at or beyond the head seen at
        // subscription time, proving it's a genuine fresh head.
        anyhow::ensure!(
            header.0.number.0 >= start.0 as i64,
            "newHeads number {} precedes the head {} seen at subscription time",
            header.0.number.0,
            start.0
        );
        Ok(())
    })
}

fn eth_subscribe_pending_transactions(tx: TestTransaction) -> RpcTestScenario {
    RpcTestScenario::basic(move |client| {
        let tx = tx.clone();
        async move {
            let (mut ws_stream, subscription_id) =
                open_eth_subscription(&client, SubscriptionKind::PendingTransactions, None).await?;

            // The subscription is active, so the pending tx we push now is observable.
            let cid = invoke_contract(&client, &tx).await?;
            let tx_hash = client
                .call(EthGetTransactionHashByCid::request((cid,))?)
                .await?
                .context("no Eth transaction hash for CID")?;

            // Watch pending-tx hashes until `our` transaction shows up.
            let watch = async {
                loop {
                    let payload = next_subscription_payload(
                        &mut ws_stream,
                        &subscription_id,
                        Duration::from_secs(120),
                    )
                    .await?;
                    // a pending tx is a single hash string.
                    anyhow::ensure!(
                        payload.is_string(),
                        "pendingTransactions must yield a tx-hash string, got: {payload}"
                    );
                    let hash: EthHash = serde_json::from_value(payload)
                        .context("pendingTransactions payload is not an Eth hash")?;
                    // received hash must be `our` transaction hash.
                    if hash.eq(&tx_hash) {
                        break;
                    }
                }
                anyhow::Ok(())
            };
            let outcome = tokio::time::timeout(Duration::from_secs(120), watch)
                .await
                .unwrap_or_else(|_| {
                    Err(anyhow::anyhow!(
                        "timed out waiting for our pendingTransactions notification"
                    ))
                });

            let _ = close_eth_subscription(&mut ws_stream, &subscription_id).await;
            outcome
        }
    })
}

/// Minimal typed view of an `EthLog` for the fields the log test asserts on.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogView {
    topics: Vec<EthHash>,
    transaction_hash: EthHash,
}

/// One opened logs subscription paired with the events it is expected to deliver.
struct CaseSub {
    label: &'static str,
    ws: EthSubStream,
    sub_id: serde_json::Value,
    /// Event signatures (each log's `topic[0]`) this filter should deliver for our mint.
    expected: Vec<EthHash>,
}

/// Read notifications until one for our mint tx (`our_tx`) arrives; unrelated logs are
/// skipped. Returns `None` if the `timeout` window elapses (or the socket closes) first.
async fn next_our_log(
    ws: &mut EthSubStream,
    sub_id: &serde_json::Value,
    our_tx: &EthHash,
    timeout: Duration,
) -> anyhow::Result<Option<LogView>> {
    while let Ok(payload) = next_subscription_payload(ws, sub_id, timeout).await {
        ensure!(
            payload.is_object(),
            "logs must yield a single log object, got: {payload}"
        );
        let log: LogView =
            serde_json::from_value(payload).context("logs payload is not an Eth log")?;
        if &log.transaction_hash == our_tx {
            return Ok(Some(log));
        }
    }
    Ok(None)
}

/// Backstop for a single subscription read. The coordinator's cancellation normally ends a
/// drain first; this only bounds a read if the mint never executes.
const LOGS_DELIVERY_TIMEOUT: Duration = Duration::from_secs(100);

/// Drain one case subscription until the coordinator signals `stop` (a settle window after the mint's receipt
/// lands), then assert the set of event signatures it delivered for our mint equals
/// `expected`. An empty `expected` asserts the filter delivered nothing.
async fn verify_case(
    label: &str,
    ws: &mut EthSubStream,
    sub_id: &serde_json::Value,
    our_tx: &EthHash,
    expected: &[EthHash],
    stop: &CancellationToken,
) -> anyhow::Result<()> {
    let mut got = Vec::new();
    loop {
        tokio::select! {
            _ = stop.cancelled() => break,
            log = next_our_log(ws, sub_id, our_tx, LOGS_DELIVERY_TIMEOUT) => {
                if let Some(log) = log? {
                    got.push(*log.topics.first().context("log is missing topic[0]")?);
                }
            }
        }
    }
    // Reorgs re-deliver the same event (apply -> revert -> re-apply); assert the *set* of
    // event signatures delivered, not the count.
    got.sort();
    got.dedup();
    let mut want = expected.to_vec();
    want.sort();
    ensure!(got == want, "{label}: expected {want:?}, got {got:?}");
    Ok(())
}

fn eth_subscribe_logs(tx: TestTransaction) -> RpcTestScenario {
    RpcTestScenario::basic(move |client| {
        // Once the txn lands, how long to let the log feed push to every subscription before we stop draining.
        const SETTLE: Duration = Duration::from_secs(10);

        let tx = tx.clone();
        async move {
            // A valid address that is not the contract.
            const WRONG_ADDRESS: &str = "0x000000000000000000000000000000000000dead";
            // The suite already passes Mint's signature as `topic`, so reuse it.
            // Transfer's signature isn't passed in derive it from the event string.
            let mint_topic = tx.topic;
            let transfer = EthHash(H256::from(crate::utils::encoding::keccak_256(
                b"Transfer(address,address,uint256)",
            )));
            let contract = EthAddress::from_filecoin_address(&tx.to)?;

            // The values we filter on come straight from the `mint(address,uint256)`
            // calldata (`--payload`): a 4-byte selector, then the 32-byte `to` and `amount`
            // words.
            let v_to = EthHash(H256::from_slice(
                tx.payload
                    .get(4..36)
                    .context("mint calldata missing `to`")?,
            ));
            let v_amount = EthHash(H256::from_slice(
                tx.payload
                    .get(36..68)
                    .context("mint calldata missing `amount`")?,
            ));
            let v_zero = EthHash::default(); // address(0)

            // Contract address as a `0x..` string, for embedding in filters.
            let c = format!("{:#x}", contract.0);

            // Every topic case scopes to the contract address; only `topics` varies.
            let by_topics =
                |topics: serde_json::Value| json!({ "address": [c.as_str()], "topics": topics });

            // Expected event sets, named for the assertion site.
            let both_topics = vec![mint_topic, transfer];
            let only_mint_topic = vec![mint_topic];
            let only_transfer_topic = vec![transfer];
            let none: Vec<EthHash> = vec![];

            // (label, filter, expected events), grouped by the dimension it exercises.
            let specs: Vec<(&'static str, serde_json::Value, Vec<EthHash>)> = vec![
                // --- address ---
                (
                    "address empty list (wildcard)",
                    json!({ "address": [], "topics": null }),
                    both_topics.clone(),
                ),
                (
                    "address [contract, other]",
                    json!({ "address": [c.as_str(), WRONG_ADDRESS], "topics": null }),
                    both_topics.clone(),
                ),
                (
                    "address non-matching",
                    json!({ "address": [WRONG_ADDRESS], "topics": null }),
                    none.clone(),
                ),
                // --- topic[0] = event signature ---
                (
                    "topic0 = Mint",
                    by_topics(json!([mint_topic.to_string()])),
                    only_mint_topic.clone(),
                ),
                (
                    "topic0 = Transfer",
                    by_topics(json!([transfer.to_string()])),
                    only_transfer_topic.clone(),
                ),
                (
                    "topic0 OR [Mint, Transfer]",
                    by_topics(json!([[mint_topic.to_string(), transfer.to_string()]])),
                    both_topics.clone(),
                ),
                (
                    "topic0 empty-list wildcard",
                    by_topics(json!([[]])),
                    both_topics.clone(),
                ),
                (
                    "topic0 null wildcard",
                    by_topics(json!([null])),
                    both_topics.clone(),
                ),
                // --- trailing wildcard / positions past the log's topics ---
                (
                    "Mint + trailing null",
                    by_topics(json!([mint_topic.to_string(), null])),
                    only_mint_topic.clone(),
                ),
                (
                    "Mint + null past topics",
                    by_topics(json!([mint_topic.to_string(), v_to.to_string(), null])),
                    only_mint_topic.clone(),
                ),
                (
                    "Mint + value past topics (no match)",
                    by_topics(json!([
                        mint_topic.to_string(),
                        v_to.to_string(),
                        v_to.to_string()
                    ])),
                    none.clone(),
                ),
                // --- indexed values: AND across positions, OR within, positional ---
                (
                    "topic1 = to (only Mint)",
                    by_topics(json!([null, v_to.to_string()])),
                    only_mint_topic.clone(),
                ),
                (
                    "topic1 = 0x0 (only Transfer)",
                    by_topics(json!([null, v_zero.to_string()])),
                    only_transfer_topic.clone(),
                ),
                (
                    "topic2 = to (only Transfer)",
                    by_topics(json!([null, null, v_to.to_string()])),
                    only_transfer_topic.clone(),
                ),
                (
                    "Transfer AND from=0 AND to",
                    by_topics(json!([
                        transfer.to_string(),
                        v_zero.to_string(),
                        v_to.to_string()
                    ])),
                    only_transfer_topic.clone(),
                ),
                (
                    "Transfer AND topic1=to (mismatch)",
                    by_topics(json!([transfer.to_string(), v_to.to_string()])),
                    none.clone(),
                ),
                (
                    "(Mint|Transfer) AND topic1=to",
                    by_topics(json!([
                        [mint_topic.to_string(), transfer.to_string()],
                        v_to.to_string()
                    ])),
                    only_mint_topic.clone(),
                ),
                // --- data is never matched as a topic ---
                (
                    "topic1 = amount (in data, no match)",
                    by_topics(json!([null, v_amount.to_string()])),
                    none.clone(),
                ),
            ];
            let mut subs = Vec::with_capacity(specs.len());
            for (label, filter, expected) in specs {
                let (ws, sub_id) =
                    open_eth_subscription(&client, SubscriptionKind::Logs, Some(filter)).await?;
                subs.push(CaseSub {
                    label,
                    ws,
                    sub_id,
                    expected,
                });
            }

            // Emit the operator's `mint` tx; it produces the Mint + Transfer logs.
            let cid = invoke_contract(&client, &tx).await?;
            let tx_hash = client
                .call(EthGetTransactionHashByCid::request((cid,))?)
                .await?
                .context("no Eth transaction hash for CID")?;
            // Drain every subscription concurrently so none sits idle while we wait.
            // All subs receive the same logs at once, so the coordinator just waits for the mint to execute,
            // and then we wait for some `SETTLE` time and then stop as soon as the logs are delivered.
            let stop = CancellationToken::new();
            // Make sure token is cancelled on error path
            let _cancellation_token_drop_guard = stop.drop_guard_ref();
            let coordinator = async {
                let executed = wait_pending_message(&client, cid).await;
                tokio::time::sleep(SETTLE).await;
                stop.cancel();
                executed
            };
            let drains =
                futures::future::join_all(subs.iter_mut().map(|c| {
                    verify_case(c.label, &mut c.ws, &c.sub_id, &tx_hash, &c.expected, &stop)
                }));
            let (executed, case_results) = tokio::join!(coordinator, drains);
            let outcome = executed.and_then(|()| {
                for result in case_results {
                    result?;
                }
                anyhow::Ok(())
            });

            for sub in &mut subs {
                let _ = close_eth_subscription(&mut sub.ws, &sub.sub_id).await;
            }
            outcome
        }
    })
}

fn create_eth_new_filter_test() -> RpcTestScenario {
    RpcTestScenario::basic(|client| async move {
        const BLOCK_RANGE: u64 = 200;

        let last_block = client.call(EthBlockNumber::request(())?).await?;

        let filter_spec = EthFilterSpec {
            from_block: Some(EthUint64(last_block.0.saturating_sub(BLOCK_RANGE)).to_hex_string()),
            to_block: Some(last_block.to_hex_string()),
            ..Default::default()
        };

        let filter_id = client.call(EthNewFilter::request((filter_spec,))?).await?;

        let removed = client
            .call(EthUninstallFilter::request((filter_id.clone(),))?)
            .await?;
        anyhow::ensure!(removed);

        let removed = client
            .call(EthUninstallFilter::request((filter_id,))?)
            .await?;
        anyhow::ensure!(!removed);

        Ok(())
    })
}

fn create_eth_new_filter_limit_test(count: usize) -> RpcTestScenario {
    RpcTestScenario::basic(move |client| async move {
        const BLOCK_RANGE: u64 = 200;

        let last_block = client.call(EthBlockNumber::request(())?).await?;

        let filter_spec = EthFilterSpec {
            from_block: Some(format!("0x{:x}", last_block.0.saturating_sub(BLOCK_RANGE))),
            to_block: Some(last_block.to_hex_string()),
            ..Default::default()
        };

        let mut ids = vec![];

        for _ in 0..count {
            let result = client
                .call(EthNewFilter::request((filter_spec.clone(),))?)
                .await;

            match result {
                Ok(filter_id) => ids.push(filter_id),
                Err(e) => {
                    // Cleanup any filters created so far to leave a clean state
                    for id in ids {
                        let removed = client.call(EthUninstallFilter::request((id,))?).await?;
                        anyhow::ensure!(removed);
                    }
                    anyhow::bail!(e)
                }
            }
        }

        for id in ids {
            let removed = client.call(EthUninstallFilter::request((id,))?).await?;
            anyhow::ensure!(removed);
        }

        Ok(())
    })
}

fn eth_new_block_filter() -> RpcTestScenario {
    RpcTestScenario::basic(move |client| async move {
        async fn process_filter(client: &rpc::Client, filter_id: &FilterID) -> anyhow::Result<()> {
            let poll = async || -> anyhow::Result<Vec<EthHash>> {
                match client
                    .call(EthGetFilterChanges::request((filter_id.clone(),))?)
                    .await?
                {
                    EthFilterResult::Hashes(hashes) => Ok(hashes),
                    _ => Err(anyhow::anyhow!("expecting block hashes")),
                }
            };
            let verify_hashes = async |hashes: &[EthHash]| -> anyhow::Result<()> {
                for hash in hashes {
                    let _block = client
                        .call(EthGetBlockByHash::request((*hash, false))?)
                        .await?;
                }
                Ok(())
            };

            let prev_hashes = poll().await?;
            verify_hashes(&prev_hashes).await?;

            // The filter derives its hashes from executed events, so an epoch
            // without events legitimately leaves the poll unchanged, allow a
            // few more epochs before declaring the filter stuck.
            let mut hashes = Vec::new();
            for _ in 0..3 {
                wait_next_epoch(client).await?;
                hashes = poll().await?;
                verify_hashes(&hashes).await?;
                if hashes != prev_hashes {
                    break;
                }
            }
            anyhow::ensure!((prev_hashes.is_empty() && hashes.is_empty()) || prev_hashes != hashes);

            Ok(())
        }

        // Create the filter
        let filter_id = client.call(EthNewBlockFilter::request(())?).await?;

        let result = process_filter(&client, &filter_id).await;

        // Cleanup
        let cleanup: anyhow::Result<()> = async {
            let removed = client
                .call(EthUninstallFilter::request((filter_id,))?)
                .await
                .context("failed to uninstall filter")?;
            anyhow::ensure!(removed, "filter was not removed");
            Ok(())
        }
        .await;

        // A cleanup failure must not mask the original test failure.
        result.and(cleanup)
    })
}

fn eth_new_pending_transaction_filter(tx: TestTransaction) -> RpcTestScenario {
    RpcTestScenario::basic(move |client| {
        let tx = tx.clone();
        async move {
            let filter_id = client
                .call(EthNewPendingTransactionFilter::request(())?)
                .await?;

            let filter_result = client
                .call(EthGetFilterChanges::request((filter_id.clone(),))?)
                .await?;

            let result = if let EthFilterResult::Hashes(prev_hashes) = filter_result {
                let cid = invoke_contract(&client, &tx).await?;
                let tx_hash = client
                    .call(EthGetTransactionHashByCid::request((cid,))?)
                    .await?
                    .context("no Eth transaction hash for CID")?;

                // Observe the mempool state before the message is mined. The
                // filter collects asynchronously, so poll until tx_hash appears.
                wait_in_mempool(&client, cid).await?;
                let hashes = poll_pending_filter_until(&client, &filter_id, &tx_hash).await?;

                anyhow::ensure!(
                    prev_hashes != hashes,
                    "prev_hashes={prev_hashes:?} hashes={hashes:?}"
                );
                anyhow::ensure!(
                    hashes.contains(&tx_hash),
                    "transaction hash missing from filter results: tx_hash={tx_hash:?} cid={cid:?} hashes={hashes:?}"
                );
                Ok(())
            } else {
                Err(anyhow::anyhow!("expecting transactions"))
            };

            let removed = client
                .call(EthUninstallFilter::request((filter_id,))?)
                .await?;
            anyhow::ensure!(removed);

            result
        }
    })
}

/// Verify that successive `eth_getFilterChanges` polls return only the
/// pending transactions added since the previous poll.
///
/// 1. Install a pending-tx filter.
/// 2. Drain any baseline state with an initial poll.
/// 3. Submit tx A, wait for it in the mempool, poll — assert hash A present.
/// 4. Submit tx B, wait for it in the mempool, poll — assert hash B present
///    and hash A absent (it was already consumed by the previous poll).
fn eth_new_pending_transaction_filter_multi_poll(tx: TestTransaction) -> RpcTestScenario {
    RpcTestScenario::basic(move |client| {
        let tx = tx.clone();
        async move {
            let filter_id = client
                .call(EthNewPendingTransactionFilter::request(())?)
                .await?;

            let result = async {
                // Baseline: clear any pre-existing pending state.
                let _ = client
                    .call(EthGetFilterChanges::request((filter_id.clone(),))?)
                    .await?;

                // First tx — poll until it shows up (collection is async).
                let cid_a = invoke_contract(&client, &tx).await?;
                let hash_a = client
                    .call(EthGetTransactionHashByCid::request((cid_a,))?)
                    .await?
                    .context("no Eth transaction hash for cid_a")?;
                wait_in_mempool(&client, cid_a).await?;
                poll_pending_filter_until(&client, &filter_id, &hash_a).await?;

                // Second tx — the next polls return it but not the
                // already-consumed tx_a.
                let cid_b = invoke_contract(&client, &tx).await?;
                let hash_b = client
                    .call(EthGetTransactionHashByCid::request((cid_b,))?)
                    .await?
                    .context("no Eth transaction hash for cid_b")?;
                wait_in_mempool(&client, cid_b).await?;
                let hashes_b = poll_pending_filter_until(&client, &filter_id, &hash_b).await?;
                anyhow::ensure!(
                    !hashes_b.contains(&hash_a),
                    "second poll should not return previously-consumed tx_a: \
                     hash_a={hash_a:?} hashes={hashes_b:?}"
                );

                anyhow::Ok(())
            }
            .await;

            let removed = client
                .call(EthUninstallFilter::request((filter_id,))?)
                .await?;
            anyhow::ensure!(removed);

            result
        }
    })
}

fn as_logs(input: EthFilterResult) -> EthFilterResult {
    match input {
        EthFilterResult::Hashes(vec) if vec.is_empty() => EthFilterResult::Logs(Vec::new()),
        other => other,
    }
}

fn eth_get_filter_logs(tx: TestTransaction) -> RpcTestScenario {
    RpcTestScenario::basic(move |client| {
        let tx = tx.clone();
        async move {
            const BLOCK_RANGE: u64 = 1;

            let tipset = client.call(ChainHead::request(())?).await?;
            let cid = invoke_contract(&client, &tx).await?;
            let lookup = client
                .call(
                    StateWaitMsg::request((cid, 1, tipset.epoch(), true))?
                        .with_timeout(Duration::from_secs(300)),
                )
                .await?;
            let block_num = EthUint64(lookup.height as u64);

            let topics = EthTopicSpec(vec![EthHashList::Single(Some(tx.topic))]);
            let filter_spec = EthFilterSpec {
                from_block: Some(format!("0x{:x}", block_num.0.saturating_sub(BLOCK_RANGE))),
                topics: Some(topics),
                ..Default::default()
            };

            let filter_id = client
                .call(EthNewFilter::request((filter_spec.clone(),))?)
                .await?;
            let filter_result = as_logs(
                client
                    .call(EthGetFilterLogs::request((filter_id.clone(),))?)
                    .await?,
            );
            let result = if let EthFilterResult::Logs(logs) = filter_result {
                anyhow::ensure!(
                    !logs.is_empty(),
                    "Empty logs: filter_spec={filter_spec:?} cid={cid:?}",
                );
                Ok(())
            } else {
                Err(anyhow::anyhow!("expecting logs"))
            };

            let removed = client
                .call(EthUninstallFilter::request((filter_id,))?)
                .await?;
            anyhow::ensure!(removed);

            result
        }
    })
}

const LOTUS_EVENTS_MAXFILTERS: usize = 100;

macro_rules! with_methods {
    ( $builder:expr, $( $method:ty ),+ ) => {{
        let mut b = $builder;
        $(
            b = b.using::<{ <$method>::N_REQUIRED_PARAMS }, $method>();
        )+
        b
    }};
}

pub(super) async fn create_tests(tx: TestTransaction) -> Vec<RpcTestScenario> {
    vec![
        with_methods!(
            create_eth_new_filter_test().name("eth_newFilter install/uninstall"),
            EthNewFilter,
            EthUninstallFilter
        ),
        with_methods!(
            create_eth_new_filter_limit_test(20).name("eth_newFilter under limit"),
            EthNewFilter,
            EthUninstallFilter
        ),
        with_methods!(
            create_eth_new_filter_limit_test(LOTUS_EVENTS_MAXFILTERS)
                .name("eth_newFilter just under limit"),
            EthNewFilter,
            EthUninstallFilter
        ),
        with_methods!(
            create_eth_new_filter_limit_test(LOTUS_EVENTS_MAXFILTERS + 1)
                .name("eth_newFilter over limit")
                .should_fail_with("maximum number of filters registered"),
            EthNewFilter,
            EthUninstallFilter
        ),
        with_methods!(
            eth_new_block_filter().name("eth_newBlockFilter works"),
            EthNewBlockFilter,
            EthGetFilterChanges,
            EthUninstallFilter
        ),
        with_methods!(
            eth_new_pending_transaction_filter(tx.clone())
                .name("eth_newPendingTransactionFilter works"),
            EthNewPendingTransactionFilter,
            EthGetFilterChanges,
            EthGetTransactionHashByCid,
            EthUninstallFilter
        ),
        with_methods!(
            eth_new_pending_transaction_filter_multi_poll(tx.clone())
                .name("eth_getFilterChanges returns only new pending txs per poll"),
            EthNewPendingTransactionFilter,
            EthGetFilterChanges,
            EthGetTransactionHashByCid,
            EthUninstallFilter
        ),
        with_methods!(
            eth_get_filter_logs(tx.clone()).name("eth_getFilterLogs works"),
            EthNewFilter,
            EthGetFilterLogs,
            EthUninstallFilter
        ),
        with_methods!(
            eth_subscribe_new_heads().name("eth_subscribe newHeads works"),
            EthSubscribe,
            EthUnsubscribe
        ),
        with_methods!(
            eth_subscribe_pending_transactions(tx.clone())
                .name("eth_subscribe pendingTransactions works"),
            EthSubscribe,
            EthUnsubscribe,
            EthGetTransactionHashByCid
        ),
        with_methods!(
            eth_subscribe_logs(tx.clone()).name("eth_subscribe logs filter matrix"),
            EthSubscribe,
            EthUnsubscribe,
            EthGetTransactionHashByCid
        ),
    ]
}

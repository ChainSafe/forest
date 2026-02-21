// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::eth::EVMMethod;
use crate::message::SignedMessage;
use crate::networks::calibnet::ETH_CHAIN_ID;
use crate::rpc::eth::EthUint64;
use crate::rpc::eth::types::*;
use crate::rpc::types::ApiTipsetKey;
use crate::rpc::{self, RpcMethod, prelude::*};
use crate::shim::{address::Address, message::Message};

use anyhow::{Context, ensure};
use cbor4ii::core::Value;
use cid::Cid;
use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

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

#[allow(unreachable_code)]
async fn next_tipset(client: &rpc::Client) -> anyhow::Result<()> {
    async fn close_channel(
        stream: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        id: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "xrpc.cancel",
            "params": [id]
        });

        stream
            .send(WsMessage::Text(request.to_string().into()))
            .await
            .context("failed to send close channel request")?;

        Ok(())
    }

    let mut url = client.base_url().clone();
    url.set_scheme("ws")
        .map_err(|_| anyhow::anyhow!("failed to set scheme"))?;
    url.set_path("/rpc/v1");

    // Note: The token is not required for the ChainNotify method.
    let (mut ws_stream, _) = connect_async(url.as_str()).await?;

    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "Filecoin.ChainNotify",
        "params": []
    });
    ws_stream
        .send(WsMessage::Text(request.to_string().into()))
        .await?;

    let mut channel_id: Option<serde_json::Value> = None;

    // The goal of this loop is to wait for a new tipset to arrive without using a busy loop or sleep.
    // It processes incoming WebSocket messages until it encounters an "apply" or "revert" change type.
    // If an "apply" change is found, it closes the channel and exits. If a "revert" change is found,
    // it closes the channel and raises an error. Any channel protocol or parameter validation issues result in an error.
    loop {
        let msg = match tokio::time::timeout(Duration::from_secs(180), ws_stream.next()).await {
            Ok(Some(msg)) => msg,
            Ok(None) => anyhow::bail!("WebSocket stream closed"),
            Err(_) => {
                if let Some(id) = channel_id.as_ref() {
                    let _ = close_channel(&mut ws_stream, id).await;
                }
                let _ = ws_stream.close(None).await;
                anyhow::bail!("timeout waiting for tipset");
            }
        };
        match msg {
            Ok(WsMessage::Text(text)) => {
                let json: serde_json::Value = serde_json::from_str(&text)?;

                if let Some(id) = json.get("result") {
                    channel_id = Some(id.clone());
                } else {
                    let method = json!("xrpc.ch.val");
                    anyhow::ensure!(json.get("method") == Some(&method));

                    if let Some(params) = json.get("params").and_then(|v| v.as_array()) {
                        if let Some(id) = params.first() {
                            anyhow::ensure!(Some(id) == channel_id.as_ref());
                        } else {
                            anyhow::bail!("expecting an open channel");
                        }
                        if let Some(changes) = params.get(1).and_then(|v| v.as_array()) {
                            for change in changes {
                                if let Some(type_) = change.get("Type").and_then(|v| v.as_str()) {
                                    if type_ == "apply" {
                                        let id = channel_id.as_ref().ok_or_else(|| {
                                            anyhow::anyhow!("subscription not opened")
                                        })?;
                                        close_channel(&mut ws_stream, id).await?;
                                        ws_stream.close(None).await?;
                                        return Ok(());
                                    } else if type_ == "revert" {
                                        let id = channel_id.as_ref().ok_or_else(|| {
                                            anyhow::anyhow!("subscription not opened")
                                        })?;
                                        close_channel(&mut ws_stream, id).await?;
                                        ws_stream.close(None).await?;
                                        anyhow::bail!("revert");
                                    }
                                }
                            }
                        }
                    } else {
                        let id = channel_id
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!("subscription not opened"))?;
                        close_channel(&mut ws_stream, id).await?;
                        ws_stream.close(None).await?;
                        anyhow::bail!("expecting params");
                    }
                }
            }
            Err(..) | Ok(WsMessage::Close(..)) => {
                let id = channel_id
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("subscription not opened"))?;
                close_channel(&mut ws_stream, id).await?;
                ws_stream.close(None).await?;
                anyhow::bail!("unexpected error or close message");
            }
            _ => {
                // Ignore other message types
            }
        }
    }

    unreachable!("loop always returns within the branches above")
}

async fn wait_pending_message(client: &rpc::Client, message_cid: Cid) -> anyhow::Result<()> {
    let tipset = client.call(ChainHead::request(())?).await?;
    let mut retries = 100;
    loop {
        let pending = client
            .call(MpoolPending::request((ApiTipsetKey(None),))?)
            .await?;

        if pending.0.iter().any(|msg| msg.cid() == message_cid) {
            client
                .call(
                    StateWaitMsg::request((message_cid, 1, tipset.epoch(), true))?
                        .with_timeout(Duration::from_secs(300)),
                )
                .await?;
            break Ok(());
        }
        ensure!(retries != 0, "Message not found in mpool");
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
        .unsigned_message(&unsigned_msg.message)?
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build EIP-1559 transaction: {}", e))?;
    let eth_tx = crate::eth::EthTx::Eip1559(Box::new(eth_tx_args));
    let data = eth_tx.rlp_unsigned_message(ETH_CHAIN_ID)?;

    let sig = client.call(WalletSign::request((tx.from, data))?).await?;
    let smsg = SignedMessage::new_unchecked(unsigned_msg.message, sig);
    let cid = smsg.cid();

    client.call(MpoolPush::request((smsg,))?).await?;

    Ok(cid)
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
            let filter_result = client
                .call(EthGetFilterChanges::request((filter_id.clone(),))?)
                .await?;

            if let EthFilterResult::Hashes(prev_hashes) = filter_result {
                let verify_hashes = async |hashes: &[EthHash]| -> anyhow::Result<()> {
                    for hash in hashes {
                        let _block = client
                            .call(EthGetBlockByHash::request((*hash, false))?)
                            .await?;
                    }
                    Ok(())
                };
                verify_hashes(&prev_hashes).await?;

                // Wait for the next block to arrive
                next_tipset(client).await?;

                let filter_result = client
                    .call(EthGetFilterChanges::request((filter_id.clone(),))?)
                    .await?;

                if let EthFilterResult::Hashes(hashes) = filter_result {
                    verify_hashes(&hashes).await?;
                    anyhow::ensure!(
                        (prev_hashes.is_empty() && hashes.is_empty()) || prev_hashes != hashes,
                    );

                    Ok(())
                } else {
                    Err(anyhow::anyhow!("expecting blocks"))
                }
            } else {
                Err(anyhow::anyhow!("expecting blocks"))
            }
        }

        let mut retries = 5;
        loop {
            // Create the filter
            let filter_id = client.call(EthNewBlockFilter::request(())?).await?;

            let result = match process_filter(&client, &filter_id).await {
                Ok(()) => Ok(()),
                Err(e) if retries != 0 && e.to_string().contains("revert") => {
                    // Cleanup
                    let removed = client
                        .call(EthUninstallFilter::request((filter_id,))?)
                        .await?;
                    anyhow::ensure!(removed);

                    retries -= 1;
                    continue;
                }
                Err(e) => Err(e),
            };

            // Cleanup
            let removed = client
                .call(EthUninstallFilter::request((filter_id,))?)
                .await?;
            anyhow::ensure!(removed);

            break result;
        }
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

                wait_pending_message(&client, cid).await?;

                let filter_result = client
                    .call(EthGetFilterChanges::request((filter_id.clone(),))?)
                    .await?;

                if let EthFilterResult::Hashes(hashes) = filter_result {
                    anyhow::ensure!(
                        prev_hashes != hashes,
                        "prev_hashes={prev_hashes:?} hashes={hashes:?}"
                    );

                    let mut cids = vec![];
                    for hash in &hashes {
                        if let Some(cid) = client
                            .call(EthGetMessageCidByTransactionHash::request((*hash,))?)
                            .await?
                        {
                            cids.push(cid);
                        }
                    }

                    anyhow::ensure!(
                        cids.contains(&cid),
                        "CID missing from filter results: cid={cid:?} cids={cids:?} hashes={hashes:?}"
                    );

                    Ok(())
                } else {
                    Err(anyhow::anyhow!("expecting hashes"))
                }
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
            EthUninstallFilter
        ),
        with_methods!(
            eth_get_filter_logs(tx.clone()).name("eth_getFilterLogs works"),
            EthNewFilter,
            EthGetFilterLogs,
            EthUninstallFilter
        ),
    ]
}

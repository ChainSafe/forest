// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::eth::EVMMethod;
use crate::rpc::eth::EthUint64;
use crate::rpc::eth::types::*;
use crate::rpc::types::ApiTipsetKey;
use crate::rpc::{self, RpcMethod, prelude::*};
use crate::shim::{address::Address, message::Message};

use anyhow::Context;
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

    fn ignore(mut self, msg: &'static str) -> Self {
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
    if let Some(token) = client.token() {
        println!("token: {token}");
    }

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
                println!("ok");
                passed += 1;
            }
            Err(e) => {
                if let Some(expected_msg) = test.should_fail_with {
                    let err_str = format!("{e:#}");
                    if err_str.contains(expected_msg) {
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
    Ok(())
}

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
    url.set_path("rpc/v1");

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
    while let Some(msg) = ws_stream.next().await {
        if let Ok(WsMessage::Text(text)) = msg {
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
                                    close_channel(&mut ws_stream, &channel_id.unwrap()).await?;
                                    ws_stream.close(None).await?;
                                    return Ok(());
                                } else if type_ == "revert" {
                                    close_channel(&mut ws_stream, &channel_id.unwrap()).await?;
                                    ws_stream.close(None).await?;
                                    anyhow::bail!("revert");
                                }
                            }
                        }
                    }
                } else {
                    close_channel(&mut ws_stream, &channel_id.unwrap()).await?;
                    ws_stream.close(None).await?;
                    anyhow::bail!("expecting params");
                }
            }
        }
    }

    anyhow::bail!("WebSocket stream closed")
}

async fn wait_pending_message(client: &rpc::Client, message_cid: Cid) -> anyhow::Result<()> {
    let mut retries = 10;
    loop {
        let pending = client
            .call(MpoolPending::request((ApiTipsetKey(None),))?)
            .await?;

        if pending.0.iter().any(|msg| msg.cid() == message_cid) {
            break Ok(());
        }
        if retries == 0 {
            anyhow::bail!("Message not found in mpool");
        }
        retries -= 1;

        tokio::time::sleep(Duration::from_millis(10)).await;
    }
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
                            .call(EthGetBlockByHash::request((hash.clone(), false))?)
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
                    anyhow::ensure!(prev_hashes != hashes);

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
                let encoded = cbor4ii::serde::to_vec(
                    Vec::with_capacity(tx.payload.len()),
                    &Value::Bytes(tx.payload.clone()),
                )
                .context("failed to encode params")?;

                let message = Message {
                    to: tx.to,
                    from: tx.from,
                    method_num: EVMMethod::InvokeContract as u64,
                    params: encoded.into(),
                    ..Default::default()
                };

                let smsg = client
                    .call(MpoolPushMessage::request((message, None))?)
                    .await?;

                wait_pending_message(&client, smsg.cid()).await?;

                let filter_result = client
                    .call(EthGetFilterChanges::request((filter_id.clone(),))?)
                    .await?;

                if let EthFilterResult::Hashes(hashes) = filter_result {
                    anyhow::ensure!(prev_hashes != hashes);

                    let mut cids = vec![];
                    for hash in hashes {
                        if let Some(cid) = client
                            .call(EthGetMessageCidByTransactionHash::request((hash,))?)
                            .await?
                        {
                            cids.push(cid);
                        }
                    }

                    anyhow::ensure!(cids.contains(&smsg.cid()));

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

            let encoded = cbor4ii::serde::to_vec(
                Vec::with_capacity(tx.payload.len()),
                &Value::Bytes(tx.payload.clone()),
            )
            .context("failed to encode params")?;

            let message = Message {
                to: tx.to,
                from: tx.from,
                method_num: EVMMethod::InvokeContract as u64,
                params: encoded.into(),
                ..Default::default()
            };

            let smsg = client
                .call(MpoolPushMessage::request((message, None))?)
                .await?;

            let lookup = client
                .call(
                    StateWaitMsg::request((smsg.cid(), 0, tipset.epoch(), false))?
                        .with_timeout(Duration::MAX),
                )
                .await?;

            let block_num = EthUint64(lookup.height as u64);

            let topics = EthTopicSpec(vec![EthHashList::Single(Some(tx.topic))]);

            let filter_spec = EthFilterSpec {
                from_block: Some(format!("0x{:x}", block_num.0 - BLOCK_RANGE)),
                to_block: Some(block_num.to_hex_string()),
                topics: Some(topics),
                ..Default::default()
            };

            let filter_id = client.call(EthNewFilter::request((filter_spec,))?).await?;

            let filter_result = as_logs(
                client
                    .call(EthGetFilterLogs::request((filter_id.clone(),))?)
                    .await?,
            );

            let result = if let EthFilterResult::Logs(logs) = filter_result {
                anyhow::ensure!(!logs.is_empty());
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
                .should_fail_with("maximum number of filters registered")
                .ignore("https://github.com/ChainSafe/forest/issues/5915"),
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
                .name("eth_newPendingTransactionFilter works")
                .ignore("https://github.com/ChainSafe/forest/issues/5916"),
            EthNewPendingTransactionFilter,
            EthGetFilterChanges,
            EthUninstallFilter
        ),
        with_methods!(
            eth_get_filter_logs(tx.clone())
                .name("eth_getFilterLogs works")
                .ignore("https://github.com/ChainSafe/forest/issues/5917"),
            EthNewFilter,
            EthGetFilterLogs,
            EthUninstallFilter
        ),
    ]
}

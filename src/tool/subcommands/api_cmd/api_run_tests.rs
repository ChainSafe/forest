// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::eth::EVMMethod;
use crate::rpc::eth::types::*;
use crate::rpc::{self, RpcMethod, prelude::*};
use crate::shim::clock::EPOCH_DURATION_SECONDS;
use crate::shim::{address::Address, message::Message};
use std::io::{self, Write};
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Context;
use cbor4ii::core::Value;
use tokio::time::{Duration, sleep};

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
}

#[derive(Clone)]
pub struct RpcTestScenario {
    pub run: TestRunner,
    pub name: Option<&'static str>,
    pub should_fail_with: Option<&'static str>,
    pub used_methods: Vec<&'static str>,
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
    let ignored = 0;
    let mut filtered = 0;

    println!("running {} tests", tests.clone().into_iter().count());

    for (i, test) in tests.into_iter().enumerate() {
        if !filter.is_empty() && !test.used_methods.iter().any(|m| m.starts_with(&filter)) {
            filtered += 1;
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

fn create_eth_new_filter_test() -> RpcTestScenario {
    RpcTestScenario::basic(|client| async move {
        const BLOCK_RANGE: u64 = 200;

        let last_block = client.call(EthBlockNumber::request(())?).await?;

        let filter_spec = EthFilterSpec {
            from_block: Some(format!("0x{:x}", last_block.0 - BLOCK_RANGE)),
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
            from_block: Some(format!("0x{:x}", last_block.0 - BLOCK_RANGE)),
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
        let filter_id = client.call(EthNewBlockFilter::request(())?).await?;

        let filter_result = client
            .call(EthGetFilterChanges::request((filter_id.clone(),))?)
            .await?;

        let result = if let EthFilterResult::Hashes(prev_hashes) = filter_result {
            let verify_hashes = async |hashes: &[EthHash]| {
                for hash in hashes {
                    let _block = client
                        .call(EthGetBlockByHash::request((hash.clone(), false))?)
                        .await?;
                }
                Ok::<(), crate::rpc::ClientError>(())
            };
            verify_hashes(&prev_hashes).await?;

            // Wait till the next block arrive
            sleep(Duration::from_secs(EPOCH_DURATION_SECONDS as u64)).await;

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
        };

        let removed = client
            .call(EthUninstallFilter::request((filter_id,))?)
            .await?;
        anyhow::ensure!(removed);

        result
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

                sleep(Duration::from_secs(2)).await;

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
            const BLOCK_RANGE: u64 = 200;

            let last_block = client.call(EthBlockNumber::request(())?).await?;

            let filter_spec = EthFilterSpec {
                from_block: Some(format!("0x{:x}", last_block.0 - BLOCK_RANGE)),
                to_block: Some(last_block.to_hex_string()),
                ..Default::default()
            };

            let filter_id = client.call(EthNewFilter::request((filter_spec,))?).await?;

            let filter_result = as_logs(
                client
                    .call(EthGetFilterLogs::request((filter_id.clone(),))?)
                    .await?,
            );

            let result = if let EthFilterResult::Logs(prev_logs) = filter_result {
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

                let _smsg = client
                    .call(MpoolPushMessage::request((message, None))?)
                    .await?;

                sleep(Duration::from_secs(2)).await;

                let filter_result = as_logs(
                    client
                        .call(EthGetFilterLogs::request((filter_id.clone(),))?)
                        .await?,
                );

                if let EthFilterResult::Logs(logs) = filter_result {
                    anyhow::ensure!(prev_logs != logs);

                    Ok(())
                } else {
                    Err(anyhow::anyhow!("expecting logs"))
                }
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

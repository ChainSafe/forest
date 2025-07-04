// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::eth::{EthChainId as EthChainIdType, SAFE_EPOCH_DELAY};
use crate::lotus_json::HasLotusJson;
use crate::message::{Message as _, SignedMessage};
use crate::rpc::eth::{
    BlockNumberOrHash, EthInt64, ExtBlockNumberOrHash, ExtPredefined, Predefined,
    new_eth_tx_from_signed_message, types::*,
};
use crate::rpc::{self, prelude::*};
use std::pin::Pin;
use std::sync::Arc;

pub struct RpcTestScenario {
    pub run: Arc<
        dyn Fn(Arc<rpc::Client>) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>
            + Send
            + Sync,
    >,
    pub ignore: Option<&'static str>,
}

impl RpcTestScenario {
    fn ignore(mut self, msg: &'static str) -> Self {
        self.ignore = Some(msg);
        self
    }
}

/// eth_newFilter -> poll with eth_getFilterChanges
/// eth_uninstallFilter
/// eth_newPendingTransactionFilter -> poll with eth_getFilterChanges
/// eth_newBlockFilter -> poll with eth_getFilterChanges
/// eth_getFilterLogs -> get all at once
/// eth_getFilterChanges

fn create_eth_new_filter_test() -> RpcTestScenario {
    RpcTestScenario {
        run: Arc::new(|client: Arc<rpc::Client>| {
            Box::pin(async move {
                const BLOCK_RANGE: u64 = 200;

                // Get the last block number
                let last_block = client.call(EthBlockNumber::request(())?).await?;

                // Create filter spec
                let filter_spec = EthFilterSpec {
                    from_block: Some(format!("0x{:x}", last_block.0 - BLOCK_RANGE)),
                    to_block: Some(last_block.to_hex_string()),
                    ..Default::default()
                };

                // Create new filter
                let filter_id = client.call(EthNewFilter::request((filter_spec,))?).await?;

                // Uninstall the filter - should succeed
                let removed = client
                    .call(EthUninstallFilter::request((filter_id.clone(),))?)
                    .await?;
                anyhow::ensure!(removed);

                // Try uninstalling again - should fail
                let removed = client
                    .call(EthUninstallFilter::request((filter_id,))?)
                    .await?;
                anyhow::ensure!(removed == false);

                Ok(())
            })
        }),
        ignore: None,
    }
}

pub(super) async fn create_tests() -> anyhow::Result<Vec<RpcTestScenario>> {
    let mut tests = vec![];

    tests.push(create_eth_new_filter_test());

    Ok(tests)
}

pub(super) async fn run_tests(
    tests: impl IntoIterator<Item = RpcTestScenario>,
    forest: impl Into<Arc<rpc::Client>>,
    lotus: impl Into<Arc<rpc::Client>>,
) -> anyhow::Result<()> {
    let lotus = Into::<Arc<rpc::Client>>::into(lotus);

    for (i, test) in tests.into_iter().enumerate() {
        println!("Running test #{}...", i);

        let result = (test.run)(lotus.clone()).await;

        match result {
            Ok(_) => {
                println!("Test #{} passed.", i);
            }
            Err(e) => {
                eprintln!("Test #{} failed: {:#}", i, e);
            }
        }
    }

    Ok(())
}

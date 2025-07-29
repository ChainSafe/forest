// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::eth::types::{EthAddressList, EthTopicSpec};
use jsonrpsee::proc_macros::rpc;
use serde::{Deserialize, Serialize};

#[rpc(server, namespace = "eth")]
pub trait EthPubSubApi {
    /// Subscribe to Ethereum events
    #[subscription(
        name = "subscribe" => "subscription",
        aliases = ["Filecoin.EthSubscribe"],
        unsubscribe = "unsubscribe",
        unsubscribe_aliases = ["Filecoin.EthUnsubscribe"],
        item = serde_json::Value
    )]
    async fn subscribe(
        &self,
        kind: SubscriptionKind,
        params: Option<SubscriptionParams>,
    ) -> jsonrpsee::core::SubscriptionResult;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubscriptionKind {
    NewHeads,
    PendingTransactions,
    Logs,
}

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LogFilter {
    pub address: EthAddressList,
    pub topics: Option<EthTopicSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionParams {
    #[serde(flatten)]
    pub filter: Option<LogFilter>,
}

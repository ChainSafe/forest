// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use prometheus::{
    core::{AtomicU64, GenericCounter, GenericCounterVec, Opts},
    Error as PrometheusError, Histogram, HistogramOpts, Registry,
};

pub mod labels {
    // gosssipsub_message_total
    pub const HELLO_REQUEST: &str = "hello_request";
    pub const PEER_CONNECTED: &str = "peer_connected";
    pub const PEER_DISCONNECTED: &str = "peer_disconnected";
    pub const PUBSUB_BLOCK: &str = "pubsub_message_block";
    pub const PUBSUB_MESSAGE: &str = "pubsub_message_message";
    pub const CHAIN_EXCHANGE_REQUEST: &str = "chain_exchange_request";
    pub const BITSWAP_BLOCK: &str = "bitswap_block";
}

#[derive(Clone)]
pub struct Metrics {
    pub tipset_processing_time: Box<Histogram>,
    pub gossipsub_message_total: Box<GenericCounterVec<AtomicU64>>,
    pub invalid_tipset_total: Box<GenericCounter<AtomicU64>>,
    pub tipset_range_sync_failure_total: Box<GenericCounter<AtomicU64>>,
}

impl Metrics {
    pub fn register(registry: &Registry) -> Result<Self, PrometheusError> {
        let tipset_processing_time = Box::new(Histogram::with_opts(HistogramOpts {
            common_opts: Opts::new(
                "tipset_processing_time",
                "Duration of routine which processes Tipsets to include them in the store",
            ),
            buckets: vec![],
        })?);
        let gossipsub_message_total = Box::new(GenericCounterVec::<AtomicU64>::new(
            Opts::new(
                "gossipsub_messsage_total",
                "Total number of gossipsub message by type",
            ),
            &[
                labels::HELLO_REQUEST,
                labels::PEER_CONNECTED,
                labels::PEER_DISCONNECTED,
                labels::PUBSUB_BLOCK,
                labels::PUBSUB_MESSAGE,
                labels::CHAIN_EXCHANGE_REQUEST,
                labels::BITSWAP_BLOCK,
            ],
        )?);
        let invalid_tipset_total = Box::new(GenericCounter::<AtomicU64>::new(
            "invalid_tipset_total",
            "Total number of invalid tipsets received over gossipsub",
        )?);
        let tipset_range_sync_failure_total = Box::new(GenericCounter::<AtomicU64>::new(
            "tipset_range_sync_failure_total",
            "Total number of errors produced by TipsetRangeSyncers",
        )?);

        registry.register(tipset_processing_time.clone())?;
        registry.register(gossipsub_message_total.clone())?;
        registry.register(invalid_tipset_total.clone())?;
        registry.register(tipset_range_sync_failure_total.clone())?;

        Ok(Self {
            tipset_processing_time,
            gossipsub_message_total,
            invalid_tipset_total,
            tipset_range_sync_failure_total,
        })
    }
}

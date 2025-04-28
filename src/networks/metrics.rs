// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use prometheus_client::{collector::Collector, encoding::EncodeMetric, metrics::gauge::Gauge};

use super::calculate_expected_epoch;

#[derive(Debug)]
pub struct NetworkHeightCollector {
    block_delay_secs: u32,
    genesis_timestamp: u64,
    network_height: Gauge,
}

impl NetworkHeightCollector {
    pub fn new(block_delay_secs: u32, genesis_timestamp: u64) -> Self {
        Self {
            block_delay_secs,
            genesis_timestamp,
            network_height: Gauge::default(),
        }
    }
}

impl Collector for NetworkHeightCollector {
    fn encode(
        &self,
        mut encoder: prometheus_client::encoding::DescriptorEncoder,
    ) -> Result<(), std::fmt::Error> {
        let metric_encoder = encoder.encode_descriptor(
            "expected_network_height",
            "The expected network height based on the current time and the genesis block time",
            None,
            self.network_height.metric_type(),
        )?;

        let expected_epoch = calculate_expected_epoch(
            chrono::Utc::now().timestamp() as u64,
            self.genesis_timestamp,
            self.block_delay_secs,
        );
        self.network_height.set(expected_epoch);
        self.network_height.encode(metric_encoder)?;

        Ok(())
    }
}

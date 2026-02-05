// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use educe::Educe;
use prometheus_client::{
    collector::Collector,
    encoding::{DescriptorEncoder, EncodeMetric},
    metrics::gauge::Gauge,
};

use super::calculate_expected_epoch;
use crate::{networks::ChainConfig, shim::clock::ChainEpoch};

#[derive(Educe, derive_more::Constructor)]
#[educe(Debug)]
pub struct NetworkHeightCollector<F>
where
    F: Fn() -> ChainEpoch,
{
    block_delay_secs: u32,
    genesis_timestamp: u64,
    #[educe(Debug(ignore))]
    get_chain_head_height: Arc<F>,
}

impl<F> Collector for NetworkHeightCollector<F>
where
    F: Fn() -> ChainEpoch + Send + Sync + 'static,
{
    fn encode(
        &self,
        mut encoder: prometheus_client::encoding::DescriptorEncoder,
    ) -> Result<(), std::fmt::Error> {
        {
            let network_height: Gauge = Default::default();
            let epoch = (self.get_chain_head_height)();
            network_height.set(epoch);
            let metric_encoder = encoder.encode_descriptor(
                "head_epoch",
                "Latest epoch synchronized to the node",
                None,
                network_height.metric_type(),
            )?;
            network_height.encode(metric_encoder)?;
        }
        {
            let expected_network_height: Gauge = Default::default();
            let expected_epoch = calculate_expected_epoch(
                chrono::Utc::now().timestamp() as u64,
                self.genesis_timestamp,
                self.block_delay_secs,
            );
            expected_network_height.set(expected_epoch);
            let metric_encoder = encoder.encode_descriptor(
                "expected_network_height",
                "The expected network height based on the current time and the genesis block time",
                None,
                expected_network_height.metric_type(),
            )?;
            expected_network_height.encode(metric_encoder)?;
        }
        Ok(())
    }
}

#[derive(Educe, derive_more::Constructor)]
#[educe(Debug)]
pub struct NetworkVersionCollector<F1, F2>
where
    F1: Fn() -> ChainEpoch,
    F2: Fn() -> u64,
{
    chain_config: Arc<ChainConfig>,
    #[educe(Debug(ignore))]
    get_chain_head_height: Arc<F1>,
    #[educe(Debug(ignore))]
    get_chain_head_actor_version: Arc<F2>,
}

impl<F1, F2> Collector for NetworkVersionCollector<F1, F2>
where
    F1: Fn() -> ChainEpoch + Send + Sync + 'static,
    F2: Fn() -> u64 + Send + Sync + 'static,
{
    fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
        let epoch = (self.get_chain_head_height)();
        {
            let network_version = self.chain_config.network_version(epoch);
            let nv_gauge: Gauge = Default::default();
            nv_gauge.set(u32::from(network_version) as _);
            let metric_encoder = encoder.encode_descriptor(
                "network_version",
                "Network version of the current chain head",
                None,
                nv_gauge.metric_type(),
            )?;
            nv_gauge.encode(metric_encoder)?;
        }
        {
            let network_version_revision = self.chain_config.network_version_revision(epoch);
            let nv_gauge: Gauge = Default::default();
            nv_gauge.set(network_version_revision);
            let metric_encoder = encoder.encode_descriptor(
                "network_version_revision",
                "Network version revision of the current chain head",
                None,
                nv_gauge.metric_type(),
            )?;
            nv_gauge.encode(metric_encoder)?;
        }
        {
            let actor_version = (self.get_chain_head_actor_version)();
            let av_gauge: Gauge = Default::default();
            av_gauge.set(actor_version as _);
            let metric_encoder = encoder.encode_descriptor(
                "actor_version",
                "Actor version of the current chain head",
                None,
                av_gauge.metric_type(),
            )?;
            av_gauge.encode(metric_encoder)?;
        }
        Ok(())
    }
}

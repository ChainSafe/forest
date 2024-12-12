// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use libp2p::PeerId;
use once_cell::sync::Lazy;
use prometheus_client::{
    encoding::{EncodeLabelKey, EncodeLabelSet, EncodeLabelValue, LabelSetEncoder},
    metrics::{counter::Counter, family::Family, gauge::Gauge},
};

pub static PEER_FAILURE_TOTAL: Lazy<Counter> = Lazy::new(|| {
    let metric = Counter::default();
    crate::metrics::default_registry().register(
        "peer_failure_total",
        "Total number of failed peer requests",
        metric.clone(),
    );
    metric
});

pub static FULL_PEERS: Lazy<Gauge> = Lazy::new(|| {
    let metric = Gauge::default();
    crate::metrics::default_registry().register(
        "full_peers",
        "Number of healthy peers recognized by the node",
        metric.clone(),
    );
    metric
});

pub static BAD_PEERS: Lazy<Gauge> = Lazy::new(|| {
    let metric = Gauge::default();
    crate::metrics::default_registry().register(
        "bad_peers",
        "Number of bad peers recognized by the node",
        metric.clone(),
    );
    metric
});

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct PeerLabel(PeerId);

impl PeerLabel {
    pub const fn new(peer: PeerId) -> Self {
        Self(peer)
    }
}

impl EncodeLabelSet for PeerLabel {
    fn encode(&self, mut encoder: LabelSetEncoder) -> Result<(), std::fmt::Error> {
        let mut label_encoder = encoder.encode_label();
        let mut label_key_encoder = label_encoder.encode_label_key()?;
        EncodeLabelKey::encode(&"PEER", &mut label_key_encoder)?;
        let mut label_value_encoder = label_key_encoder.encode_label_value()?;
        EncodeLabelValue::encode(&self.0.to_string(), &mut label_value_encoder)?;
        label_value_encoder.finish()
    }
}

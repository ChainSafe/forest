// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use once_cell::sync::Lazy;
use prometheus_client::{
    encoding::{EncodeLabelKey, EncodeLabelSet, EncodeLabelValue, LabelSetEncoder},
    metrics::{counter::Counter, family::Family, gauge::Gauge, histogram::Histogram},
};

use crate::metrics::TypeLabel;

pub static TIPSET_PROCESSING_TIME: Lazy<Histogram> = Lazy::new(|| {
    let metric = crate::metrics::default_histogram();
    crate::metrics::default_registry().register(
        "tipset_processing_time",
        "Duration of routine which processes Tipsets to include them in the store",
        metric.clone(),
    );
    metric
});
pub static BLOCK_VALIDATION_TIME: Lazy<Histogram> = Lazy::new(|| {
    let metric = crate::metrics::default_histogram();
    crate::metrics::default_registry().register(
        "block_validation_time",
        "Duration of routine which validate blocks with no cache hit",
        metric.clone(),
    );
    metric
});
pub static BLOCK_VALIDATION_TASKS_TIME: Lazy<Family<TypeLabel, Histogram>> = Lazy::new(|| {
    let metric = Family::new_with_constructor(crate::metrics::default_histogram as _);
    crate::metrics::default_registry().register(
        "block_validation_tasks_time",
        "Duration of subroutines inside block validation",
        metric.clone(),
    );
    metric
});
pub static LIBP2P_MESSAGE_TOTAL: Lazy<Family<Libp2pMessageKindLabel, Counter>> = Lazy::new(|| {
    let metric = Family::default();
    crate::metrics::default_registry().register(
        "libp2p_messsage_total",
        "Total number of libp2p messages by type",
        metric.clone(),
    );
    metric
});
pub static INVALID_TIPSET_TOTAL: Lazy<Counter> = Lazy::new(|| {
    let metric = Counter::default();
    crate::metrics::default_registry().register(
        "invalid_tipset_total",
        "Total number of invalid tipsets received over gossipsub",
        metric.clone(),
    );
    metric
});
pub static TIPSET_RANGE_SYNC_FAILURE_TOTAL: Lazy<Counter> = Lazy::new(|| {
    let metric = Counter::default();
    crate::metrics::default_registry().register(
        "tipset_range_sync_failure_total",
        "Total number of errors produced by TipsetRangeSyncers",
        metric.clone(),
    );
    metric
});
pub static HEAD_EPOCH: Lazy<Gauge> = Lazy::new(|| {
    let metric = Gauge::default();
    crate::metrics::default_registry().register(
        "head_epoch",
        "Latest epoch synchronized to the node",
        metric.clone(),
    );
    metric
});
pub static LAST_VALIDATED_TIPSET_EPOCH: Lazy<Gauge> = Lazy::new(|| {
    let metric = Gauge::default();
    crate::metrics::default_registry().register(
        "last_validated_tipset_epoch",
        "Last validated tipset epoch",
        metric.clone(),
    );
    metric
});
pub static NETWORK_HEAD_EVALUATION_ERRORS: Lazy<Counter> = Lazy::new(|| {
    let metric = Counter::default();
    crate::metrics::default_registry().register(
        "network_head_evaluation_errors",
        "Total number of network head evaluation errors",
        metric.clone(),
    );
    metric
});
pub static BOOTSTRAP_ERRORS: Lazy<Counter> = Lazy::new(|| {
    let metric = Counter::default();
    crate::metrics::default_registry().register(
        "bootstrap_errors",
        "Total number of bootstrap attempts failures",
        metric.clone(),
    );
    metric
});
pub static FOLLOW_NETWORK_INTERRUPTIONS: Lazy<Counter> = Lazy::new(|| {
    let metric = Counter::default();
    crate::metrics::default_registry().register(
        "follow_network_interruptions",
        "Total number of follow network interruptions, where it unexpectedly ended",
        metric.clone(),
    );
    metric
});
pub static FOLLOW_NETWORK_ERRORS: Lazy<Counter> = Lazy::new(|| {
    let metric = Counter::default();
    crate::metrics::default_registry().register(
        "follow_network_errors",
        "Total number of follow network errors",
        metric.clone(),
    );
    metric
});

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Libp2pMessageKindLabel(&'static str);

impl Libp2pMessageKindLabel {
    pub const fn new(kind: &'static str) -> Self {
        Self(kind)
    }
}

impl EncodeLabelSet for Libp2pMessageKindLabel {
    fn encode(&self, mut encoder: LabelSetEncoder) -> Result<(), std::fmt::Error> {
        let mut label_encoder = encoder.encode_label();
        let mut label_key_encoder = label_encoder.encode_label_key()?;
        EncodeLabelKey::encode(&"libp2p_message_kind", &mut label_key_encoder)?;
        let mut label_value_encoder = label_key_encoder.encode_label_value()?;
        EncodeLabelValue::encode(&self.0, &mut label_value_encoder)?;
        label_value_encoder.finish()
    }
}

pub mod values {
    use super::Libp2pMessageKindLabel;
    use crate::metrics::TypeLabel;

    // libp2p_message_total
    pub const HELLO_REQUEST_INBOUND: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("hello_request_in");
    pub const HELLO_RESPONSE_OUTBOUND: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("hello_response_out");
    pub const HELLO_REQUEST_OUTBOUND: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("hello_request_out");
    pub const HELLO_RESPONSE_INBOUND: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("hello_response_in");
    pub const PEER_CONNECTED: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("peer_connected");
    pub const PEER_DISCONNECTED: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("peer_disconnected");
    pub const PUBSUB_BLOCK: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("pubsub_message_block");
    pub const PUBSUB_MESSAGE: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("pubsub_message_message");
    pub const CHAIN_EXCHANGE_REQUEST_OUTBOUND: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("chain_exchange_request_out");
    pub const CHAIN_EXCHANGE_RESPONSE_INBOUND: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("chain_exchange_response_in");
    pub const CHAIN_EXCHANGE_REQUEST_INBOUND: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("chain_exchange_request_in");
    pub const CHAIN_EXCHANGE_RESPONSE_OUTBOUND: Libp2pMessageKindLabel =
        Libp2pMessageKindLabel::new("chain_exchange_response_out");

    // block validation tasks
    pub const BASE_FEE_CHECK: TypeLabel = TypeLabel::new("base_fee_check");
    pub const PARENT_WEIGHT_CAL: TypeLabel = TypeLabel::new("parent_weight_check");
    pub const BLOCK_SIGNATURE_CHECK: TypeLabel = TypeLabel::new("block_signature_check");
}

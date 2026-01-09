// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use parking_lot::MappedRwLockReadGuard;
use prometheus_client::{
    encoding::EncodeLabelSet,
    metrics::{counter::Counter, family::Family, gauge::Gauge, histogram::Histogram},
    registry::Registry,
};
use std::sync::LazyLock;

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct TypeLabel {
    r#type: &'static str,
}

impl TypeLabel {
    fn new(t: &'static str) -> Self {
        Self { r#type: t }
    }
}

static MESSAGE_COUNTER: LazyLock<Family<TypeLabel, Counter>> = LazyLock::new(Default::default);
static CONTAINER_CAPACITIES: LazyLock<Family<TypeLabel, Gauge>> = LazyLock::new(Default::default);
pub(in crate::libp2p_bitswap) static GET_BLOCK_TIME: LazyLock<Histogram> = LazyLock::new(|| {
    Histogram::new([
        0.1, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0,
    ])
});

/// Register bitswap metrics
pub fn register_metrics(registry: &mut Registry) {
    registry.register(
        "bitswap_message_count",
        "Number of bitswap messages",
        MESSAGE_COUNTER.clone(),
    );
    registry.register(
        "bitswap_container_capacities",
        "Capacity for each bitswap container",
        CONTAINER_CAPACITIES.clone(),
    );
    registry.register(
        "bitswap_get_block_time",
        "Duration of get_block",
        GET_BLOCK_TIME.clone(),
    );
}

pub(in crate::libp2p_bitswap) fn inbound_stream_count<'a>() -> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new("inbound_stream_count"))
}

pub(in crate::libp2p_bitswap) fn outbound_stream_count<'a>() -> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new("outbound_stream_count"))
}

pub(in crate::libp2p_bitswap) fn message_counter_get_block_success<'a>()
-> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new("get_block_success"))
}

pub(in crate::libp2p_bitswap) fn message_counter_get_block_failure<'a>()
-> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new("get_block_failure"))
}

pub(in crate::libp2p_bitswap) fn message_counter_inbound_request_have<'a>()
-> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new("inbound_request_have"))
}

pub(in crate::libp2p_bitswap) fn message_counter_inbound_request_block<'a>()
-> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new("inbound_request_block"))
}

pub(in crate::libp2p_bitswap) fn message_counter_outbound_request_cancel<'a>()
-> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new("outbound_request_cancel"))
}

pub(in crate::libp2p_bitswap) fn message_counter_outbound_request_block<'a>()
-> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new("outbound_request_block"))
}

pub(in crate::libp2p_bitswap) fn message_counter_outbound_request_have<'a>()
-> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new("outbound_request_have"))
}

pub(in crate::libp2p_bitswap) fn message_counter_inbound_response_have_yes<'a>()
-> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new("inbound_response_have_yes"))
}

pub(in crate::libp2p_bitswap) fn message_counter_inbound_response_have_no<'a>()
-> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new("inbound_response_have_no"))
}

pub(in crate::libp2p_bitswap) fn message_counter_inbound_response_block<'a>()
-> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new("inbound_response_block"))
}

pub(in crate::libp2p_bitswap) fn message_counter_inbound_response_block_update_db<'a>()
-> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new("inbound_response_block_update_db"))
}

pub(in crate::libp2p_bitswap) fn message_counter_inbound_response_block_already_exists_in_db<'a>()
-> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new(
        "inbound_response_block_already_exists_in_db",
    ))
}

pub(in crate::libp2p_bitswap) fn message_counter_inbound_response_block_not_requested<'a>()
-> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new("inbound_response_block_not_requested"))
}

pub(in crate::libp2p_bitswap) fn message_counter_inbound_response_block_update_db_failure<'a>()
-> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new("inbound_response_block_update_db_failure"))
}

pub(in crate::libp2p_bitswap) fn message_counter_outbound_response_have<'a>()
-> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new("outbound_response_have"))
}

pub(in crate::libp2p_bitswap) fn message_counter_outbound_response_block<'a>()
-> MappedRwLockReadGuard<'a, Counter> {
    MESSAGE_COUNTER.get_or_create(&TypeLabel::new("outbound_response_block"))
}

pub(in crate::libp2p_bitswap) fn peer_container_capacity<'a>() -> MappedRwLockReadGuard<'a, Gauge> {
    CONTAINER_CAPACITIES.get_or_create(&TypeLabel::new("peer_container_capacity"))
}

pub(in crate::libp2p_bitswap) fn response_channel_container_capacity<'a>()
-> MappedRwLockReadGuard<'a, Gauge> {
    CONTAINER_CAPACITIES.get_or_create(&TypeLabel::new("response_channel_container_capacity"))
}

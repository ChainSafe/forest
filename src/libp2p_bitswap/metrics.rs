// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use lazy_static::lazy_static;
use prometheus::{core::*, *};

lazy_static! {
    static ref MESSAGE_SIZE: IntCounterVec = IntCounterVec::new(
        Opts::new("bitswap_message_size", "Size of bitswap messages",),
        &["type"],
    )
    .expect("Infallible");
    static ref MESSAGE_COUNTER: IntCounterVec = IntCounterVec::new(
        Opts::new("bitswap_message_count", "Number of bitswap messages",),
        &["type"],
    )
    .expect("Infallible");
    static ref CONTAINER_CAPACITIES: GenericGaugeVec<AtomicU64> = GenericGaugeVec::new(
        Opts::new(
            "bitswap_container_capacities",
            "Capacity for each bitswap container",
        ),
        &["type"],
    )
    .expect("Infallible");
    pub(crate) static ref GET_BLOCK_TIME: Histogram = Histogram::with_opts(HistogramOpts {
        common_opts: Opts::new("bitswap_get_block_time", "Duration of get_block"),
        buckets: vec![0.1, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
    })
    .expect("Infallible");
}

/// Register bitswap metrics
pub fn register_metrics(registry: &Registry) -> anyhow::Result<()> {
    registry.register(Box::new(MESSAGE_SIZE.clone()))?;
    registry.register(Box::new(MESSAGE_COUNTER.clone()))?;
    registry.register(Box::new(CONTAINER_CAPACITIES.clone()))?;
    registry.register(Box::new(GET_BLOCK_TIME.clone()))?;

    Ok(())
}

pub(crate) fn inbound_bytes() -> GenericCounter<AtomicU64> {
    MESSAGE_SIZE.with_label_values(&["inbound_bytes"])
}

pub(crate) fn outbound_bytes() -> GenericCounter<AtomicU64> {
    MESSAGE_SIZE.with_label_values(&["outbound_bytes"])
}

pub(crate) fn inbound_stream_count() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["inbound_stream_count"])
}

pub(crate) fn outbound_stream_count() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["outbound_stream_count"])
}

pub(crate) fn message_counter_get_block_success() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["get_block_success"])
}

pub(crate) fn message_counter_get_block_failure() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["get_block_failure"])
}

pub(crate) fn message_counter_inbound_request_have() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["inbound_request_have"])
}

pub(crate) fn message_counter_inbound_request_block() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["inbound_request_block"])
}

pub(crate) fn message_counter_outbound_request_cancel() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["outbound_request_cancel"])
}

pub(crate) fn message_counter_outbound_request_block() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["outbound_request_block"])
}

pub(crate) fn message_counter_outbound_request_have() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["outbound_request_have"])
}

pub(crate) fn message_counter_inbound_response_have_yes() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["inbound_response_have_yes"])
}

pub(crate) fn message_counter_inbound_response_have_no() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["inbound_response_have_no"])
}

pub(crate) fn message_counter_inbound_response_block() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["inbound_response_block"])
}

pub(crate) fn message_counter_inbound_response_block_update_db() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["inbound_response_block_update_db"])
}

pub(crate) fn message_counter_inbound_response_block_already_exists_in_db(
) -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["inbound_response_block_already_exists_in_db"])
}

pub(crate) fn message_counter_inbound_response_block_not_requested() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["inbound_response_block_not_requested"])
}

pub(crate) fn message_counter_inbound_response_block_update_db_failure() -> GenericCounter<AtomicU64>
{
    MESSAGE_COUNTER.with_label_values(&["inbound_response_block_update_db_failure"])
}

pub(crate) fn message_counter_outbound_response_have() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["outbound_response_have"])
}

pub(crate) fn message_counter_outbound_response_block() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["outbound_response_block"])
}

pub(crate) fn peer_container_capacity() -> GenericGauge<AtomicU64> {
    CONTAINER_CAPACITIES.with_label_values(&["peer_container_capacity"])
}

pub(crate) fn response_channel_container_capacity() -> GenericGauge<AtomicU64> {
    CONTAINER_CAPACITIES.with_label_values(&["response_channel_container_capacity"])
}

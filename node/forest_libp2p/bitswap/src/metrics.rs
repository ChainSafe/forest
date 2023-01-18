use lazy_static::lazy_static;
use prometheus::{
    core::{AtomicU64, GenericCounter, GenericGauge, GenericGaugeVec},
    IntCounterVec, Opts, Registry,
};

lazy_static! {
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
}

pub fn register_metrics(registry: &Registry) -> anyhow::Result<()> {
    registry.register(Box::new(MESSAGE_COUNTER.clone()))?;
    registry.register(Box::new(CONTAINER_CAPACITIES.clone()))?;

    Ok(())
}

pub(crate) fn message_counter_inbound_request_have() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["inbound_request_have"])
}

pub(crate) fn message_counter_inbound_request_block() -> GenericCounter<AtomicU64> {
    MESSAGE_COUNTER.with_label_values(&["inbound_request_block"])
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

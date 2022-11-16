// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use lazy_static::lazy_static;
use prometheus::{
    core::{AtomicU64, GenericCounter, GenericCounterVec, GenericGauge, Opts},
    Histogram, HistogramOpts,
};

lazy_static! {
    pub static ref TIPSET_PROCESSING_TIME: Box<Histogram> = {
        let tipset_processing_time = Box::new(
            Histogram::with_opts(HistogramOpts {
                common_opts: Opts::new(
                    "tipset_processing_time",
                    "Duration of routine which processes Tipsets to include them in the store",
                ),
                buckets: vec![],
            })
            .expect("Defining the tipset_processing_time metric must succeed"),
        );
        prometheus::default_registry().register(tipset_processing_time.clone()).expect(
            "Registering the tipset_processing_time metric with the metrics registry must succeed",
        );
        tipset_processing_time
    };
    pub static ref LIBP2P_MESSAGE_TOTAL: Box<GenericCounterVec<AtomicU64>> = {
        let libp2p_message_total = Box::new(
            GenericCounterVec::<AtomicU64>::new(
                Opts::new(
                    "libp2p_messsage_total",
                    "Total number of libp2p messages by type",
                ),
                &[labels::GOSSIPSUB_MESSAGE_KIND],
            )
            .expect("Defining the libp2p_message_total metric must succeed"),
        );
        prometheus::default_registry().register(libp2p_message_total.clone()).expect(
            "Registering the libp2p_message_total metric with the metrics registry must succeed"
        );
        libp2p_message_total
    };
    pub static ref INVALID_TIPSET_TOTAL: Box<GenericCounter<AtomicU64>> = {
        let invalid_tipset_total = Box::new(
            GenericCounter::<AtomicU64>::new(
                "invalid_tipset_total",
                "Total number of invalid tipsets received over gossipsub",
            )
            .expect("Defining the invalid_tipset_total metric must succeed"),
        );
        prometheus::default_registry().register(invalid_tipset_total.clone()).expect(
            "Registering the invalid_tispet_total metric with the metrics registry must succeed"
        );
        invalid_tipset_total
    };
    pub static ref TIPSET_RANGE_SYNC_FAILURE_TOTAL: Box<GenericCounter<AtomicU64>> = {
        let tipset_range_sync_failure_total = Box::new(
            GenericCounter::<AtomicU64>::new(
                "tipset_range_sync_failure_total",
                "Total number of errors produced by TipsetRangeSyncers",
            )
            .expect("Defining the tipset_range_sync_failure_total metrics must succeed"),
        );
        prometheus::default_registry()
            .register(tipset_range_sync_failure_total.clone())
            .expect("Registering the tipset_range_sync_failure_total metric with the metrics registry must succeed");
        tipset_range_sync_failure_total
    };
    pub static ref HEAD_EPOCH: Box<GenericGauge<AtomicU64>> = {
        let head_epoch = Box::new(
            GenericGauge::<AtomicU64>::new("head_epoch", "Latest epoch synchronized to the node")
                .expect("Defining the head_epoch metric must succeed"),
        );
        prometheus::default_registry()
            .register(head_epoch.clone())
            .expect("Registering the head_epoch metric with the metrics registry must succeed");
        head_epoch
    };
    pub static ref PEER_FAILURE_TOTAL: Box<GenericCounter<AtomicU64>> = {
        let peer_failure_total = Box::new(
            GenericCounter::<AtomicU64>::new(
                "peer_failure_total",
                "Total number of failed peer requests",
            )
            .expect("Defining the peer_failure_total metric must succeed"),
        );
        prometheus::default_registry()
            .register(peer_failure_total.clone())
            .expect(
                "Registering the peer_failure_total metric with the metrics registry must succeed",
            );
        peer_failure_total
    };
    pub static ref FULL_PEERS: Box<GenericGauge<AtomicU64>> = {
        let full_peers = Box::new(
            GenericGauge::<AtomicU64>::new(
                "full_peers",
                "Number of healthy peers recognized by the node",
            )
            .expect("Defining the full_peers metric must succeed"),
        );
        prometheus::default_registry()
            .register(full_peers.clone())
            .expect("Registering the full_peers metric with the metrics registry must succeed");
        full_peers
    };
    pub static ref BAD_PEERS: Box<GenericGauge<AtomicU64>> = {
        let bad_peers = Box::new(
            GenericGauge::<AtomicU64>::new(
                "bad_peers",
                "Number of bad peers recognized by the node",
            )
            .expect("Defining the bad_peers metric must succeed"),
        );
        prometheus::default_registry()
            .register(bad_peers.clone())
            .expect("Registering the bad_peers metric with the metrics registry must succeed");
        bad_peers
    };
    pub static ref LAST_VALIDATED_TIPSET_EPOCH: Box<GenericGauge<AtomicU64>> = {
        let last_validated_tipset_epoch = Box::new(
            GenericGauge::<AtomicU64>::new(
                "last_validated_tipset_epoch",
                "Last validated tipset epoch",
            )
            .expect("Defining the last_validated_tipset_epoch metric must succeed"),
        );
        prometheus::default_registry()
            .register(last_validated_tipset_epoch.clone())
            .expect("Registering the last_validated_tipset_epoch metric with the metrics registry must succeed");
        last_validated_tipset_epoch
    };
    pub static ref NETWORK_HEAD_EVALUATION_ERRORS: Box<GenericCounter<AtomicU64>> = {
        let network_head_evaluation_errors = Box::new(
            GenericCounter::<AtomicU64>::new(
                "network_head_evaluation_errors",
                "Total number of network head evaluation errors",
            )
            .expect("Defining the network_head_evaluation_errors metric must succeed"),
        );
        prometheus::default_registry()
            .register(network_head_evaluation_errors.clone())
            .expect(
                "Registering the network_head_evaluation_errors metric with the metrics registry must succeed",
            );
        network_head_evaluation_errors
    };
    pub static ref BOOTSTRAP_ERRORS: Box<GenericCounter<AtomicU64>> = {
        let boostrap_errors = Box::new(
            GenericCounter::<AtomicU64>::new(
                "bootstrap_errors",
                "Total number of bootstrap attempts failures",
            )
            .expect("Defining the bootstrap_errors metric must succeed"),
        );
        prometheus::default_registry()
            .register(boostrap_errors.clone())
            .expect(
                "Registering the bootstrap_errors metric with the metrics registry must succeed",
            );
        boostrap_errors
    };
    pub static ref FOLLOW_NETWORK_INTERRUPTIONS: Box<GenericCounter<AtomicU64>> = {
        let follow_network_restarts = Box::new(
            GenericCounter::<AtomicU64>::new(
                "follow_network_interruptions",
                "Total number of follow network interruptions, where it unexpectedly ended",
            )
            .expect("Defining the follow_network_interruptions metric must succeed"),
        );
        prometheus::default_registry()
            .register(follow_network_restarts.clone())
            .expect(
                "Registering the follow_network_interruptions metric with the metrics registry must succeed",
            );
        follow_network_restarts
    };
    pub static ref FOLLOW_NETWORK_ERRORS: Box<GenericCounter<AtomicU64>> = {
        let follow_network_errors = Box::new(
            GenericCounter::<AtomicU64>::new(
                "follow_network_errors",
                "Total number of follow network errors",
            )
            .expect("Defining the follow_network_errors metric must succeed"),
        );
        prometheus::default_registry()
            .register(follow_network_errors.clone())
            .expect(
                "Registering the follow_network_errors metric with the metrics registry must succeed",
            );
        follow_network_errors
    };
}

pub mod labels {
    pub const GOSSIPSUB_MESSAGE_KIND: &str = "libp2p_message_kind";
}

pub mod values {
    // libp2p_message_total
    pub const HELLO_REQUEST_INBOUND: &str = "hello_request_in";
    pub const HELLO_RESPONSE_OUTBOUND: &str = "hello_response_out";
    pub const HELLO_REQUEST_OUTBOUND: &str = "hello_request_out";
    pub const HELLO_RESPONSE_INBOUND: &str = "hello_response_in";
    pub const PEER_CONNECTED: &str = "peer_connected";
    pub const PEER_DISCONNECTED: &str = "peer_disconnected";
    pub const PUBSUB_BLOCK: &str = "pubsub_message_block";
    pub const PUBSUB_MESSAGE: &str = "pubsub_message_message";
    pub const CHAIN_EXCHANGE_REQUEST_OUTBOUND: &str = "chain_exchange_request_out";
    pub const CHAIN_EXCHANGE_RESPONSE_INBOUND: &str = "chain_exchange_response_in";
    pub const CHAIN_EXCHANGE_REQUEST_INBOUND: &str = "chain_exchange_request_in";
    pub const CHAIN_EXCHANGE_RESPONSE_OUTBOUND: &str = "chain_exchange_response_out";
    pub const BITSWAP_BLOCK_REQUEST_OUTBOUND: &str = "bitswap_block_request_out";
    pub const BITSWAP_BLOCK_RESPONSE_INBOUND: &str = "bitswap_block_response_in";
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheus::core::Metric;

    macro_rules! test_counter {
        ($name:ident) => {
            let _ = $name.metric();
        };
    }

    macro_rules! test_counter_vec {
        ($name:ident) => {
            let _ = $name.with_label_values(&["label"]);
        };
    }
    #[test]
    fn metrics_defined_and_registered() {
        test_counter!(TIPSET_PROCESSING_TIME);
        test_counter_vec!(LIBP2P_MESSAGE_TOTAL);
        test_counter!(INVALID_TIPSET_TOTAL);
        test_counter!(TIPSET_RANGE_SYNC_FAILURE_TOTAL);
        test_counter!(HEAD_EPOCH);
        test_counter!(PEER_FAILURE_TOTAL);
        test_counter!(FULL_PEERS);
        test_counter!(BAD_PEERS);
        test_counter!(LAST_VALIDATED_TIPSET_EPOCH);
        test_counter!(NETWORK_HEAD_EVALUATION_ERRORS);
        test_counter!(BOOTSTRAP_ERRORS);
        test_counter!(FOLLOW_NETWORK_INTERRUPTIONS);
        test_counter!(FOLLOW_NETWORK_ERRORS);
    }
}

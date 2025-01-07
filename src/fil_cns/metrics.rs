// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use once_cell::sync::Lazy;
use prometheus_client::metrics::{family::Family, histogram::Histogram};

use crate::metrics::TypeLabel;

pub static CONSENSUS_BLOCK_VALIDATION_TIME: Lazy<Histogram> = Lazy::new(|| {
    let metric = crate::metrics::default_histogram();
    crate::metrics::default_registry().register(
        "cns_block_validation_time",
        "Duration of routine which validate blocks in fil_cns",
        metric.clone(),
    );
    metric
});
pub static CONSENSUS_BLOCK_VALIDATION_TASKS_TIME: Lazy<Family<TypeLabel, Histogram>> =
    Lazy::new(|| {
        let metric = Family::new_with_constructor(crate::metrics::default_histogram as _);
        crate::metrics::default_registry().register(
            "cns_block_validation_tasks_time",
            "Duration of subroutines inside cns block validation",
            metric.clone(),
        );
        metric
    });

pub mod values {
    use crate::metrics::TypeLabel;

    pub const VALIDATE_MINER: TypeLabel = TypeLabel::new("validate_miner");
    pub const VALIDATE_WINNER_ELECTION: TypeLabel = TypeLabel::new("validate_winner_election");
    pub const VALIDATE_TICKET_ELECTION: TypeLabel = TypeLabel::new("validate_ticket_election");
    pub const VERIFY_WINNING_POST_PROOF: TypeLabel = TypeLabel::new("verify_winning_post_proof");
}

// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use lazy_static::lazy_static;
use prometheus::{core::Opts, Histogram, HistogramOpts, HistogramVec};

lazy_static! {
    pub static ref CONSENSUS_BLOCK_VALIDATION_TIME: Box<Histogram> = {
        let cns_block_validation_time = Box::new(
            Histogram::with_opts(HistogramOpts {
                common_opts: Opts::new(
                    "cns_block_validation_time",
                    "Duration of routine which validate blocks in fil_cns",
                ),
                buckets: vec![],
            })
            .expect("Defining the cns_block_validation_time metric must succeed"),
        );
        prometheus::default_registry().register(cns_block_validation_time.clone()).expect(
            "Registering the cns_block_validation_time metric with the metrics registry must succeed",
        );
        cns_block_validation_time
    };
    pub static ref CONSENSUS_BLOCK_VALIDATION_TASKS_TIME: Box<HistogramVec> = {
        let cns_block_validation_tasks_time = Box::new(
            HistogramVec::new(
                HistogramOpts {
                    common_opts: Opts::new(
                        "cns_block_validation_tasks_time",
                        "Duration of subroutines inside cns block validation",
                    ),
                    buckets: vec![],
                },
                &["type"],
            )
            .expect("Defining the cns_block_validation_tasks_time metric must succeed"),
        );
        prometheus::default_registry().register(cns_block_validation_tasks_time.clone()).expect(
            "Registering the cns_block_validation_tasks_time metric with the metrics registry must succeed",
        );
        cns_block_validation_tasks_time
    };
}

pub mod values {
    pub const VALIDATE_MINER: &str = "validate_miner";
    pub const VALIDATE_WINNER_ELECTION: &str = "validate_winner_election";
    pub const VALIDATE_TICKET_ELECTION: &str = "validate_ticket_election";
    pub const VERIFY_WINNING_POST_PROOF: &str = "verify_winning_post_proof";
}

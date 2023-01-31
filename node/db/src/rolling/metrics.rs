// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use lazy_static::lazy_static;
use prometheus::{
    core::{AtomicI64, GenericGaugeVec},
    Opts,
};

lazy_static! {
    pub static ref ROLLING_DB_LAST_READ: Box<GenericGaugeVec<AtomicI64>> = {
        let rolling_db_last_read = Box::new(
            GenericGaugeVec::new(
                Opts::new("rolling_db_last_read", "rolling_db_last_read"),
                &["INDEX"],
            )
            .expect("Infallible"),
        );
        prometheus::default_registry()
            .register(rolling_db_last_read.clone())
            .expect("Infallible");
        rolling_db_last_read
    };
    pub static ref ROLLING_DB_LAST_WRITE: Box<GenericGaugeVec<AtomicI64>> = {
        let rolling_db_last_write = Box::new(
            GenericGaugeVec::new(
                Opts::new("rolling_db_last_write", "rolling_db_last_write"),
                &["INDEX"],
            )
            .expect("Infallible"),
        );
        prometheus::default_registry()
            .register(rolling_db_last_write.clone())
            .expect("Infallible");
        rolling_db_last_write
    };
}

// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use lazy_static::lazy_static;
use prometheus::core::{AtomicU64, GenericGauge};

lazy_static! {
    pub static ref MPOOL_MESSAGE_TOTAL: Box<GenericGauge<AtomicU64>> = {
        let mpool_message_total = Box::new(
            GenericGauge::<AtomicU64>::new(
                "mpool_message_total",
                "Total number of messages in the message pool",
            )
            .expect("Defining the mpool_message_total metric must succeed"),
        );
        prometheus::default_registry()
            .register(mpool_message_total.clone())
            .expect(
                "Registering the mpool_message_total metric with the metrics registry must succeed",
            );
        mpool_message_total
    };
}

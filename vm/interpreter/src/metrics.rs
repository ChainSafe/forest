// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use lazy_static::lazy_static;
use prometheus::core::{AtomicU64, GenericCounterVec, Opts};

lazy_static! {
    pub static ref KERNEL_OP_COUNT: Box<GenericCounterVec<AtomicU64>> = {
        let kernel_op_count = Box::new(
            GenericCounterVec::<AtomicU64>::new(
                Opts::new("kernel_op_count", "Kernel operation count"),
                &[labels::OPERATION],
            )
            .expect("Defining the kernel_op_count metric must succeed"),
        );
        prometheus::default_registry()
            .register(kernel_op_count.clone())
            .expect(
                "Registering the kernel_op_count metric with the metrics registry must succeed",
            );
        kernel_op_count
    };
    pub static ref KERNEL_OP_DURATION: Box<GenericCounterVec<AtomicU64>> = {
        let kernel_op_duration = Box::new(
            GenericCounterVec::<AtomicU64>::new(
                Opts::new(
                    "kernel_op_duration",
                    "Kernel operations total duration (nanoseconds)",
                ),
                &[labels::OPERATION],
            )
            .expect("Defining the kernel_op_duration metric must succeed"),
        );
        prometheus::default_registry()
            .register(kernel_op_duration.clone())
            .expect(
                "Registering the kernel_op_duration metric with the metrics registry must succeed",
            );
        kernel_op_duration
    };
}

pub mod labels {
    pub const OPERATION: &str = "operation";
}

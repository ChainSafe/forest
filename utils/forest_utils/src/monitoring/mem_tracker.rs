// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    sync::atomic::{self, AtomicBool, AtomicUsize},
    time::Duration,
};

use human_repr::HumanCount;
use log::info;
use memory_stats::memory_stats;

#[derive(Default)]
pub struct MemStatsTracker {
    check_interval: Duration,
    peak_physical_mem: AtomicUsize,
    cancelled: AtomicBool,
}

impl MemStatsTracker {
    pub fn new(check_interval: Duration) -> Self {
        Self {
            check_interval,
            peak_physical_mem: Default::default(),
            cancelled: Default::default(),
        }
    }

    /// A blocking loop that records peak RRS periodically
    pub async fn run_loop(&self) {
        while !self.cancelled.load(atomic::Ordering::Relaxed) {
            if let Some(usage) = memory_stats() {
                self.peak_physical_mem
                    .fetch_max(usage.physical_mem, atomic::Ordering::Relaxed);
            }
            tokio::time::sleep(self.check_interval).await;
        }
    }
}

impl Drop for MemStatsTracker {
    fn drop(&mut self) {
        self.cancelled.store(true, atomic::Ordering::Relaxed);
        info!(
            "Peak physical memory usage: {}",
            self.peak_physical_mem
                .load(atomic::Ordering::Relaxed)
                .human_count_bytes()
        );
    }
}

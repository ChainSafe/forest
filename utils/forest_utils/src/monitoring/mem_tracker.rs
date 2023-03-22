// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    sync::atomic::{self, AtomicBool, AtomicUsize},
    time::Duration,
};

use human_repr::HumanCount;
use log::info;
use memory_stats::memory_stats;

pub struct MemStatsTracker {
    check_interval: Duration,
    peak_physical_mem: AtomicUsize,
    cancelled: AtomicBool,
}

impl MemStatsTracker {
    pub fn new(check_interval: Duration) -> Self {
        assert!(check_interval > Duration::default());

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

impl Default for MemStatsTracker {
    fn default() -> Self {
        Self::new(Duration::from_millis(1000))
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

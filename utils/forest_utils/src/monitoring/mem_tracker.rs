// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    sync::{
        atomic::{self, AtomicBool, AtomicUsize},
        Arc,
    },
    time::Duration,
};

use human_repr::HumanCount;
use log::info;
use memory_stats::memory_stats;

pub struct MemStatsTracker {
    peak_physical_mem: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    check_interval: Duration,
}

impl MemStatsTracker {
    pub fn new(check_interval: Duration) -> Self {
        Self {
            check_interval,
            peak_physical_mem: Default::default(),
            cancelled: Default::default(),
        }
    }

    pub fn run_async(&self) {
        let peak_physical_mem = self.peak_physical_mem.clone();
        let cancelled = self.cancelled.clone();
        let check_interval = self.check_interval;
        tokio::spawn(async move {
            while !cancelled.load(atomic::Ordering::Relaxed) {
                if let Some(usage) = memory_stats() {
                    peak_physical_mem.fetch_max(usage.physical_mem, atomic::Ordering::Relaxed);
                }
                tokio::time::sleep(check_interval).await;
            }
        });
    }
}

impl Default for MemStatsTracker {
    fn default() -> Self {
        Self::new(Duration::from_millis(100))
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

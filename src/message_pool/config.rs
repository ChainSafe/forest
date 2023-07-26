// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use crate::{db::SettingsStore, shim::address::Address};
use fvm_ipld_encoding::from_slice;
use serde::{Deserialize, Serialize};

const MPOOL_CONFIG_KEY: &str = "/mpool/config";
const SIZE_LIMIT_LOW: i64 = 20000;
const SIZE_LIMIT_HIGH: i64 = 30000;
const PRUNE_COOLDOWN: Duration = Duration::from_secs(60); // 1 minute
const REPLACE_BY_FEE_RATIO: f64 = 1.25;
const GAS_LIMIT_OVERESTIMATION: f64 = 1.25;

/// Configuration available for the [`crate::message_pool::MessagePool`].
///
/// [MessagePool]: crate::message_pool::MessagePool
#[derive(Clone, Serialize, Deserialize)]
pub struct MpoolConfig {
    pub priority_addrs: Vec<Address>,
    pub size_limit_high: i64,
    pub size_limit_low: i64,
    pub replace_by_fee_ratio: f64,
    pub prune_cooldown: Duration,
    pub gas_limit_overestimation: f64,
}

impl Default for MpoolConfig {
    fn default() -> Self {
        Self {
            priority_addrs: vec![],
            size_limit_high: SIZE_LIMIT_HIGH,
            size_limit_low: SIZE_LIMIT_LOW,
            replace_by_fee_ratio: REPLACE_BY_FEE_RATIO,
            prune_cooldown: PRUNE_COOLDOWN,
            gas_limit_overestimation: GAS_LIMIT_OVERESTIMATION,
        }
    }
}
#[cfg(test)]
impl MpoolConfig {
    /// Saves message pool `config` to the database, to easily reload.
    pub fn save_config<DB: SettingsStore>(&self, store: &DB) -> Result<(), anyhow::Error> {
        store.write_bin(MPOOL_CONFIG_KEY, fvm_ipld_encoding::to_vec(&self)?)
    }

    /// Returns the low limit capacity of messages to allocate.
    pub fn size_limit_low(&self) -> i64 {
        self.size_limit_low
    }

    /// Returns slice of [Address]es to prioritize when selecting messages.
    pub fn priority_addrs(&self) -> &[Address] {
        &self.priority_addrs
    }
}

impl MpoolConfig {
    /// Load `config` from store, if exists. If there is no `config`, uses
    /// default.
    pub fn load_config<DB: SettingsStore>(store: &DB) -> Result<Self, anyhow::Error> {
        match store.read_bin(MPOOL_CONFIG_KEY)? {
            Some(v) => Ok(from_slice(&v)?),
            None => Ok(Default::default()),
        }
    }
}

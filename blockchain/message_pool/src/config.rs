// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use db::Store;
use encoding::{from_slice, to_vec};
use serde::{Deserialize, Serialize};
use std::error::Error as StdError;
use std::time::Duration;

const MPOOL_CONFIG_KEY: &[u8] = b"/mpool/config";
const SIZE_LIMIT_LOW: i64 = 20000;
const SIZE_LIMIT_HIGH: i64 = 30000;
const PRUNE_COOLDOWN: Duration = Duration::from_secs(60); // 1 minute
const REPLACE_BY_FEE_RATIO: f64 = 1.25;
const GAS_LIMIT_OVERESTIMATION: f64 = 1.25;

/// Config available for the [MessagePool].
///
/// [MessagePool]: crate::MessagePool
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

impl MpoolConfig {
    pub fn new(
        priority_addrs: Vec<Address>,
        size_limit_high: i64,
        size_limit_low: i64,
        replace_by_fee_ratio: f64,
        prune_cooldown: Duration,
        gas_limit_overestimation: f64,
    ) -> Result<Self, String> {
        // Validate if parameters are valid
        if replace_by_fee_ratio < REPLACE_BY_FEE_RATIO {
            return Err(format!(
                "replace_by_fee_ratio:{} is less than required: {}",
                replace_by_fee_ratio, REPLACE_BY_FEE_RATIO
            ));
        }
        if gas_limit_overestimation < 1.0 {
            return Err(format!(
                "gas_limit_overestimation of: {} is less than required: {}",
                gas_limit_overestimation, 1
            ));
        }
        Ok(Self {
            priority_addrs,
            size_limit_high,
            size_limit_low,
            replace_by_fee_ratio,
            prune_cooldown,
            gas_limit_overestimation,
        })
    }

    /// Saves message pool config to the database, to easily reload.
    pub fn save_config<DB: Store>(&self, store: &DB) -> Result<(), Box<dyn StdError>> {
        Ok(store.write(MPOOL_CONFIG_KEY, to_vec(&self)?)?)
    }

    /// Load config from store, if exists. If there is no config, uses default.
    pub fn load_config<DB: Store>(store: &DB) -> Result<Self, Box<dyn StdError>> {
        match store.read(MPOOL_CONFIG_KEY)? {
            Some(v) => Ok(from_slice(&v)?),
            None => Ok(Default::default()),
        }
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

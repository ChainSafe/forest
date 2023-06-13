// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use async_trait::async_trait;
use byteorder::{BigEndian, ByteOrder};
use forest_shim::version::NetworkVersion;
use forest_utils::encoding::blake2b_256;

use crate::{Beacon, BeaconEntry};

/// Mock beacon used for testing. Deterministic based on an interval.
pub struct MockBeacon {
    interval: Duration,
}

impl MockBeacon {
    pub fn new(interval: Duration) -> Self {
        MockBeacon { interval }
    }
    fn entry_for_index(index: u64) -> BeaconEntry {
        let mut buf = [0; 8];
        BigEndian::write_u64(&mut buf, index);
        let rval = blake2b_256(&buf);
        BeaconEntry::new(index, rval.to_vec())
    }
    pub fn round_time(&self) -> Duration {
        self.interval
    }
}

#[async_trait]
impl Beacon for MockBeacon {
    fn verify_entry(&self, curr: &BeaconEntry, prev: &BeaconEntry) -> Result<bool, anyhow::Error> {
        let oe = Self::entry_for_index(prev.round());
        Ok(oe.data() == curr.data())
    }

    async fn entry(&self, round: u64) -> Result<BeaconEntry, anyhow::Error> {
        Ok(Self::entry_for_index(round))
    }

    fn max_beacon_round_for_epoch(&self, _network_version: NetworkVersion, fil_epoch: i64) -> u64 {
        fil_epoch as u64
    }
}

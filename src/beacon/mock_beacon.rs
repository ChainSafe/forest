// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::DrandNetwork;
use crate::beacon::{Beacon, BeaconEntry};
use crate::shim::version::NetworkVersion;
use crate::utils::encoding::blake2b_256;
use async_trait::async_trait;
use byteorder::{BigEndian, ByteOrder};

#[derive(Default)]
pub struct MockBeacon {}

impl MockBeacon {
    fn entry_for_index(index: u64) -> BeaconEntry {
        let mut buf = [0; 8];
        BigEndian::write_u64(&mut buf, index);
        let rval = blake2b_256(&buf);
        BeaconEntry::new(index, rval.to_vec())
    }
}

#[async_trait]
impl Beacon for MockBeacon {
    fn network(&self) -> DrandNetwork {
        DrandNetwork::Mainnet
    }

    fn verify_entries<'a>(
        &self,
        entries: &'a [BeaconEntry],
        mut prev: &'a BeaconEntry,
    ) -> Result<bool, anyhow::Error> {
        for curr in entries.iter() {
            let oe = Self::entry_for_index(prev.round());
            if oe.signature() != curr.signature() {
                return Ok(false);
            }

            prev = curr;
        }

        Ok(true)
    }

    async fn entry(&self, round: u64) -> Result<BeaconEntry, anyhow::Error> {
        Ok(Self::entry_for_index(round))
    }

    fn max_beacon_round_for_epoch(&self, _network_version: NetworkVersion, fil_epoch: i64) -> u64 {
        fil_epoch as u64
    }
}

// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::{
    blocks::{Tipset, TipsetKey},
    lotus_json::{base64_standard, lotus_json_with_self},
    utils::clock::Clock,
};
use cid::{multihash::MultihashDigest as _, Cid};
use fvm_shared4::ActorID;
use itertools::Itertools as _;
use once_cell::sync::Lazy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, marker::PhantomData};

/// TipSetKey is the canonically ordered concatenation of the block CIDs in a tipset.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct F3TipSetKey(
    #[schemars(with = "String")]
    #[serde(with = "base64_standard")]
    pub Vec<u8>,
);
lotus_json_with_self!(F3TipSetKey);

impl From<&TipsetKey> for F3TipSetKey {
    fn from(tsk: &TipsetKey) -> Self {
        let bytes = tsk.iter().flat_map(|cid| cid.to_bytes()).collect();
        Self(bytes)
    }
}

impl From<TipsetKey> for F3TipSetKey {
    fn from(tsk: TipsetKey) -> Self {
        (&tsk).into()
    }
}

impl TryFrom<F3TipSetKey> for TipsetKey {
    type Error = anyhow::Error;

    fn try_from(tsk: F3TipSetKey) -> Result<Self, Self::Error> {
        static BLOCK_HEADER_CID_LEN: Lazy<usize> = Lazy::new(|| {
            let buf = [0_u8; 256];
            let cid = Cid::new_v1(
                fvm_ipld_encoding::DAG_CBOR,
                cid::multihash::Code::Blake2b256.digest(&buf),
            );
            cid.to_bytes().len()
        });

        let cids: Vec<Cid> = tsk
            .0
            .chunks(*BLOCK_HEADER_CID_LEN)
            .map(Cid::read_bytes)
            .try_collect()?;

        Ok(nunny::Vec::new(cids)
            .map_err(|_| anyhow::anyhow!("tipset key cannot be empty"))?
            .into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct F3TipSet {
    pub key: F3TipSetKey,
    /// The verifiable oracle randomness used to elect this block's author leader
    #[schemars(with = "String")]
    #[serde(with = "base64_standard")]
    pub beacon: Vec<u8>,
    /// The period in which a new block is generated.
    /// There may be multiple rounds in an epoch.
    pub epoch: ChainEpoch,
    /// Block creation time, in seconds since the Unix epoch
    pub timestamp: u64,
}
lotus_json_with_self!(F3TipSet);

impl From<Tipset> for F3TipSet {
    fn from(ts: Tipset) -> Self {
        let key = ts.key().into();
        let beacon = {
            let entries = &ts.block_headers().first().beacon_entries;
            if let Some(last) = entries.last() {
                last.signature().to_vec()
            } else {
                vec![0; 32]
            }
        };
        let epoch = ts.epoch();
        let timestamp = ts.block_headers().first().timestamp;
        Self {
            key,
            beacon,
            epoch,
            timestamp,
        }
    }
}

impl From<Arc<Tipset>> for F3TipSet {
    fn from(ts: Arc<Tipset>) -> Self {
        Arc::unwrap_or_clone(ts).into()
    }
}

/// PowerEntry represents a single entry in the PowerTable, including ActorID and its StoragePower and PubKey.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Eq, PartialEq)]
pub struct F3PowerEntry {
    #[serde(rename = "ID")]
    pub id: ActorID,
    #[schemars(with = "String")]
    #[serde(rename = "Power", with = "crate::lotus_json::stringify")]
    pub power: num::BigInt,
    #[schemars(with = "String")]
    #[serde(rename = "PubKey", with = "base64_standard")]
    pub pub_key: Vec<u8>,
}
lotus_json_with_self!(F3PowerEntry);

/// Entries are sorted descending order of their power, where entries with equal power are
/// sorted by ascending order of their ID.
/// This ordering is guaranteed to be stable, since a valid PowerTable cannot contain entries with duplicate IDs
impl Ord for F3PowerEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        match other.power.cmp(&self.power) {
            Ordering::Equal => self.id.cmp(&other.id),
            ord => ord,
        }
    }
}

impl PartialOrd for F3PowerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug)]
pub struct F3LeaseManager<CLOCK: Clock<chrono::Utc> = chrono::Utc>(
    RwLock<HashMap<u64, chrono::DateTime<chrono::Utc>>>,
    PhantomData<CLOCK>,
);

impl<CLOCK: Clock<chrono::Utc>> F3LeaseManager<CLOCK> {
    pub fn get_active_participants(&self) -> HashSet<u64> {
        let now = CLOCK::now();
        self.0
            .read()
            .iter()
            .filter_map(|(id, expire)| if expire > &now { Some(*id) } else { None })
            .collect()
    }

    pub fn upsert_defensive(
        &self,
        id: u64,
        new_lease_expiration: chrono::DateTime<chrono::Utc>,
        old_lease_expiration: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<bool> {
        // Use a single now to avoid weird conditions
        let now = CLOCK::now();
        anyhow::ensure!(
            new_lease_expiration <= now + chrono::Duration::minutes(5),
            "F3 participation lease cannot be over 5 mins"
        );
        anyhow::ensure!(
            new_lease_expiration >= now,
            "F3 participation lease is in the past"
        );

        // if the old lease is expired just insert a new one
        if old_lease_expiration < now {
            self.0.write().insert(id, new_lease_expiration);
            return Ok(true);
        }

        let Some(old_lease_expiration_in_record) = self.0.read().get(&id).cloned() else {
            // we don't know about it, don't start a new lease
            return Ok(false);
        };
        if old_lease_expiration_in_record != old_lease_expiration {
            // the lease we know about does not match and because the old lease is not expired
            // we should not allow for new lease
            return Ok(false);
        }
        // we know about the lease, update it
        self.0.write().insert(id, new_lease_expiration);

        Ok(true)
    }
}

impl<CLOCK: Clock<chrono::Utc>> Default for F3LeaseManager<CLOCK> {
    fn default() -> Self {
        Self(Default::default(), Default::default())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    #[test]
    fn test_f3_lease_manager_upsert() {
        static NOW: Lazy<Mutex<chrono::DateTime<chrono::Utc>>> =
            Lazy::new(|| Mutex::new(chrono::Utc::now()));

        // Mock the clock with a static NOW
        #[derive(Debug, Default)]
        struct TestClock;

        impl Clock<chrono::Utc> for TestClock {
            fn now() -> chrono::DateTime<chrono::Utc> {
                *NOW.lock()
            }
        }

        let lm = F3LeaseManager::<TestClock>::default();
        // inserting a new lease
        let timestamp0 = chrono::DateTime::from_timestamp(0, 0).unwrap();
        let now = TestClock::now();
        let expiration1 = now + chrono::Duration::milliseconds(100);
        let miner = 1;
        assert!(lm.upsert_defensive(miner, expiration1, timestamp0).unwrap());
        // We have one active participants
        assert_eq!(lm.get_active_participants().len(), 1);
        // updating an existing lease
        let expiration2 = expiration1 + chrono::Duration::milliseconds(100);
        // failure, old lease does not match
        assert!(!lm
            .upsert_defensive(miner, expiration2, expiration2)
            .unwrap());
        // success, old lease matches
        assert!(lm
            .upsert_defensive(miner, expiration2, expiration1)
            .unwrap());
        let expiration3 = expiration2 + chrono::Duration::milliseconds(100);
        // success, old lease has expired
        assert!(lm.upsert_defensive(miner, expiration3, timestamp0).unwrap());
        // we still have one active participants
        assert_eq!(lm.get_active_participants().len(), 1);
        // sleep for 0.5s to let all leases expire
        *NOW.lock() += chrono::Duration::milliseconds(500);
        // we should have no active participants
        assert_eq!(lm.get_active_participants().len(), 0);
        // it should fail when lease is too long
        lm.upsert_defensive(
            miner,
            TestClock::now() + chrono::Duration::minutes(6),
            timestamp0,
        )
        .unwrap_err();
        // it should fail when expiration is in the past
        lm.upsert_defensive(
            miner,
            TestClock::now() - chrono::Duration::minutes(1),
            timestamp0,
        )
        .unwrap_err();
    }
}

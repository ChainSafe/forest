// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::message::SignedMessage;
use crate::message_pool::mpool_locker::KeyedLock;
use hashlink::LruCache;
use parking_lot::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::OwnedMutexGuard;
use uuid::Uuid;

/// Max retained records; prefer unlocked victims under pressure.
const MAX_RECORDS: usize = 8192;

/// Covers miner push retries (~15s back-off) with margin. Not durable across restart.
const RECORD_TTL: Duration = Duration::from_secs(60);

/// Proof that the caller holds the per-UUID lock for this UUID.
pub struct MsgUuidGuard {
    uuid: Uuid,
    _guard: OwnedMutexGuard<()>,
}

/// Signed message stored for a `MsgUuid`.
#[derive(Clone, Debug)]
pub struct MsgUuidRecord {
    pub message: SignedMessage,
    recorded_at: Instant,
}

/// Recent successful `MpoolPushMessage` results by `MsgUuid` (in-memory, TTL + cap).
/// Outermost push lock (then [`MpoolLocker`](super::MpoolLocker), then
/// [`NonceTracker`](super::NonceTracker)); hold from lookup through [`Self::record`].
pub struct MsgUuidRecords {
    locks: KeyedLock<Uuid>,
    records: Mutex<LruCache<Uuid, MsgUuidRecord>>,
}

impl Default for MsgUuidRecords {
    fn default() -> Self {
        Self {
            locks: KeyedLock::default(),
            // Unbounded LRU; capacity/TTL pruned in get/record.
            #[allow(clippy::disallowed_methods)] // exact LRU; SizeTrackingCache is CLOCK-PRO
            records: Mutex::new(LruCache::new_unbounded()),
        }
    }
}

impl MsgUuidRecords {
    /// Hold through lookup / [`Self::record`].
    pub async fn take_lock(&self, uuid: Uuid) -> MsgUuidGuard {
        MsgUuidGuard {
            uuid,
            _guard: self.locks.take_lock(uuid).await,
        }
    }

    pub fn get(&self, lock: &MsgUuidGuard) -> Option<MsgUuidRecord> {
        let mut records = self.records.lock();
        self.prune_expired(&mut records);
        records.get(&lock.uuid).cloned()
    }

    /// Returns `false` when at capacity and no unlocked victim can be evicted.
    pub fn record(&self, lock: &MsgUuidGuard, smsg: &SignedMessage) -> bool {
        self.record_at(lock, smsg, Instant::now())
    }

    fn record_at(&self, lock: &MsgUuidGuard, smsg: &SignedMessage, recorded_at: Instant) -> bool {
        let mut records = self.records.lock();
        self.prune_expired(&mut records);
        if !self.evict_unlocked_to_capacity(&mut records, lock.uuid) {
            return false;
        }
        records.insert(
            lock.uuid,
            MsgUuidRecord {
                message: smsg.clone(),
                recorded_at,
            },
        );
        true
    }

    fn prune_expired(&self, records: &mut LruCache<Uuid, MsgUuidRecord>) {
        let expired: Vec<Uuid> = records
            .iter()
            .filter(|(uuid, rec)| {
                rec.recorded_at.elapsed() > RECORD_TTL && !self.locks.is_held(uuid)
            })
            .map(|(uuid, _)| *uuid)
            .collect();
        for uuid in expired {
            // Re-check: another task may have taken the lock since the scan.
            if !self.locks.is_held(&uuid) {
                records.remove(&uuid);
            }
        }
    }

    /// Evict unlocked entries until under [`MAX_RECORDS`]. Returns `false` if
    /// still at capacity (all remaining keys locked / `keep`).
    fn evict_unlocked_to_capacity(
        &self,
        records: &mut LruCache<Uuid, MsgUuidRecord>,
        keep: Uuid,
    ) -> bool {
        while records.len() >= MAX_RECORDS {
            let victim = records.iter().find_map(|(uuid, _)| {
                if *uuid != keep && !self.locks.is_held(uuid) {
                    Some(*uuid)
                } else {
                    None
                }
            });
            match victim {
                Some(uuid) if !self.locks.is_held(&uuid) => {
                    records.remove(&uuid);
                }
                Some(_) => continue, // raced into held; try another
                None => return false,
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shim::message::Message;

    fn mock_smsg() -> SignedMessage {
        SignedMessage::mock_bls_signed_message(Message::default())
    }

    fn stale_instant() -> Instant {
        Instant::now()
            .checked_sub(RECORD_TTL + Duration::from_secs(1))
            .expect("monotonic clock old enough for TTL test")
    }

    async fn seed(records: &MsgUuidRecords, uuid: Uuid, smsg: &SignedMessage) -> MsgUuidGuard {
        let lock = records.take_lock(uuid).await;
        assert!(records.record(&lock, smsg));
        lock
    }

    #[tokio::test]
    async fn ttl_expiry_drops_record() {
        let records = MsgUuidRecords::default();
        let smsg = mock_smsg();
        let uuid = Uuid::from_u128(1);
        let lock = records.take_lock(uuid).await;
        assert!(records.record_at(&lock, &smsg, stale_instant()));
        drop(lock);
        // Prune under another UUID so the expired entry is unlocked.
        let other = records.take_lock(Uuid::from_u128(2)).await;
        let _ = records.get(&other);
        drop(other);
        let lock = records.take_lock(uuid).await;
        assert!(records.get(&lock).is_none());
    }

    #[tokio::test]
    async fn ttl_does_not_drop_held_uuid() {
        let records = MsgUuidRecords::default();
        let smsg = mock_smsg();
        let uuid = Uuid::from_u128(1);
        let lock = records.take_lock(uuid).await;
        assert!(records.record_at(&lock, &smsg, stale_instant()));
        assert!(
            records.get(&lock).is_some(),
            "held UUID must not be TTL-pruned"
        );
    }

    #[tokio::test]
    async fn locked_uuid_survives_capacity_pressure() {
        let records = MsgUuidRecords::default();
        let smsg = mock_smsg();
        let pin_lock = seed(&records, Uuid::from_u128(1), &smsg).await;

        for i in 2..=MAX_RECORDS as u128 + 2 {
            drop(seed(&records, Uuid::from_u128(i), &smsg).await);
        }

        assert!(
            records.get(&pin_lock).is_some(),
            "locked UUID must not be evicted under capacity pressure"
        );
    }

    #[tokio::test]
    async fn record_fails_when_at_cap_and_all_locked() {
        let records = MsgUuidRecords::default();
        let smsg = mock_smsg();
        let mut guards = Vec::new();
        for i in 1..=MAX_RECORDS as u128 {
            guards.push(seed(&records, Uuid::from_u128(i), &smsg).await);
        }
        let lock = records
            .take_lock(Uuid::from_u128(MAX_RECORDS as u128 + 1))
            .await;
        assert!(
            !records.record(&lock, &smsg),
            "must not grow past cap when every entry is locked"
        );
        drop(guards);
    }
}

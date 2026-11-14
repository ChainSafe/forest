// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::message::SignedMessage;
use crate::message_pool::mpool_locker::KeyedLock;
use hashlink::LinkedHashMap;
use parking_lot::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::OwnedMutexGuard;
use uuid::Uuid;

/// Max retained records; oldest LRU entries drop under pressure.
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

/// Recent successful `MpoolPushMessage` results by `MsgUuid` (in-memory, `TTL` + cap).
/// Per-UUID lock is the outermost push lock (then [`MpoolLocker`](super::MpoolLocker), then
/// [`NonceTracker`](super::NonceTracker)); hold from lookup through [`Self::record`].
pub struct MsgUuidRecords {
    locks: KeyedLock<Uuid>,
    records: Mutex<LinkedHashMap<Uuid, MsgUuidRecord>>,
    capacity: usize,
}

impl Default for MsgUuidRecords {
    fn default() -> Self {
        Self::with_capacity(MAX_RECORDS)
    }
}

impl MsgUuidRecords {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            locks: KeyedLock::default(),
            records: Mutex::new(LinkedHashMap::new()),
            capacity,
        }
    }

    /// Hold through lookup / [`Self::record`].
    pub async fn take_lock(&self, uuid: Uuid) -> MsgUuidGuard {
        MsgUuidGuard {
            uuid,
            _guard: self.locks.take_lock(uuid).await,
        }
    }

    pub fn get(&self, lock: &MsgUuidGuard) -> Option<MsgUuidRecord> {
        let mut records = self.records.lock();
        match records.get(&lock.uuid) {
            Some(rec) if rec.recorded_at.elapsed() > RECORD_TTL => {
                records.remove(&lock.uuid);
                None
            }
            Some(rec) => Some(rec.clone()),
            None => None,
        }
    }

    pub fn record(&self, lock: &MsgUuidGuard, smsg: &SignedMessage) {
        self.record_at(lock, smsg, Instant::now())
    }

    fn record_at(&self, lock: &MsgUuidGuard, smsg: &SignedMessage, recorded_at: Instant) {
        let mut records = self.records.lock();
        records.insert(
            lock.uuid,
            MsgUuidRecord {
                message: smsg.clone(),
                recorded_at,
            },
        );
        while records.len() > self.capacity {
            records.pop_front();
        }
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

    #[tokio::test]
    async fn ttl_expiry_drops_record() {
        let records = MsgUuidRecords::default();
        let smsg = mock_smsg();
        let stale = Uuid::from_u128(1);
        let fresh = Uuid::from_u128(2);
        let stale_lock = records.take_lock(stale).await;
        records.record_at(&stale_lock, &smsg, stale_instant());
        drop(stale_lock);
        let fresh_lock = records.take_lock(fresh).await;
        records.record(&fresh_lock, &smsg);
        drop(fresh_lock);

        let stale_lock = records.take_lock(stale).await;
        assert!(records.get(&stale_lock).is_none());
        let fresh_lock = records.take_lock(fresh).await;
        assert!(
            records.get(&fresh_lock).is_some(),
            "fresh UUID must survive TTL prune of a sibling"
        );
    }

    #[tokio::test]
    async fn capacity_evicts_oldest() {
        let records = MsgUuidRecords::with_capacity(2);
        let smsg = mock_smsg();
        for i in 1..=3 {
            let lock = records.take_lock(Uuid::from_u128(i)).await;
            records.record(&lock, &smsg);
        }
        let first = records.take_lock(Uuid::from_u128(1)).await;
        assert!(
            records.get(&first).is_none(),
            "oldest UUID must be evicted at capacity"
        );
        let last = records.take_lock(Uuid::from_u128(3)).await;
        assert!(records.get(&last).is_some());
    }
}

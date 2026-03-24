// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use ahash::HashMap;
use parking_lot::RwLock as SeqRwLock;
use tokio::sync::{OwnedMutexGuard, RwLock};

use crate::shim::address::Address;

use super::Error;

/// Tracks the next sequence number for each sender for RPC nonce assignment,
/// held in memory for the lifetime of the node process, and serializes
/// concurrent pushes per sender.
pub struct NonceStore {
    /// Per-sender lower bound on the next nonce to hand out (one past the last assigned).
    nonces: SeqRwLock<HashMap<Address, u64>>,
    sender_locks: RwLock<HashMap<Address, Arc<tokio::sync::Mutex<()>>>>,
}

impl NonceStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            nonces: SeqRwLock::new(HashMap::default()),
            sender_locks: RwLock::new(Default::default()),
        })
    }

    pub async fn lock_sender(&self, addr: &Address) -> OwnedMutexGuard<()> {
        let mutex = {
            let mut map = self.sender_locks.write().await;
            map.entry(*addr)
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        tokio::sync::Mutex::lock_owned(mutex).await
    }

    pub(crate) fn save_nonce(&self, addr: &Address, seq: u64) -> anyhow::Result<()> {
        self.nonces.write().insert(*addr, seq);
        Ok(())
    }

    pub(crate) fn next_nonce(&self, addr: &Address, seq: u64) -> Result<u64, Error> {
        let stored = self.nonces.read().get(addr).copied().unwrap_or(0);
        Ok(stored.max(seq))
    }
}

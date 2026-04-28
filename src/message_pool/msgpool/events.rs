// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Event types published by the pending pool.

use crate::message::SignedMessage;

pub(in crate::message_pool) const MPOOL_UPDATE_CHANNEL_CAPACITY: usize = 256;

/// A change to the pending pool.
#[allow(dead_code)] // payloads consumed by external subscribers.
#[derive(Clone, Debug)]
pub enum MpoolUpdate {
    Add(SignedMessage),
    Remove(SignedMessage),
}

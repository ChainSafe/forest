// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Message-pool update events.
//!
//! A single [`MpoolUpdate`] stream is published by [`super::pending_store::PendingStore`]
//! and consumed by observers such as the pending-transaction subscription RPC,
//! metrics exporters, and future F3 integrations.

use crate::message::SignedMessage;

/// Capacity of the [`tokio::sync::broadcast`] channel carrying [`MpoolUpdate`]s.
///
/// Messages are only buffered until every active receiver has consumed them,
/// so the capacity bounds how far behind a slow subscriber may fall before
/// they start receiving [`tokio::sync::broadcast::error::RecvError::Lagged`].
pub(in crate::message_pool) const MPOOL_UPDATE_CHANNEL_CAPACITY: usize = 256;

/// A single mutation of the pending pool.
/// Emitted exactly once per successful insert or remove.
#[allow(dead_code)] // payloads consumed by external subscribers.
#[derive(Clone, Debug)]
pub enum MpoolUpdate {
    /// A message was inserted into pending (fresh insert or `RBF` replacement).
    Add(SignedMessage),
    /// A message was removed from pending (applied on-chain or pruned).
    Remove(SignedMessage),
}

// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
mod block_prob;
mod config;
mod errors;
mod mpool_locker;
mod msg_chain;
mod msg_uuid_records;
mod msgpool;
mod nonce_tracker;

pub(crate) use self::{
    config::*,
    errors::*,
    mpool_locker::MpoolLocker,
    msg_uuid_records::MsgUuidRecords,
    msgpool::{msg_pool::MessagePool, *},
    nonce_tracker::NonceTracker,
    utils::compute_rbf,
};

pub use block_prob::block_probabilities;

/// Shared `MpoolPushMessage` locking / UUID-record state.
#[derive(Default)]
pub struct MpoolPushGuards {
    pub mpool_locker: MpoolLocker,
    pub nonce_tracker: NonceTracker,
    pub msg_uuid_records: MsgUuidRecords,
}

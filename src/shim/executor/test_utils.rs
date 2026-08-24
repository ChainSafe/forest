// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Test-only constructors and fixtures for the shim executor types.

use super::*;

impl Receipt {
    /// A successful, empty receipt at the latest supported FVM version. Lets tests build receipts
    /// without naming a concrete FVM version.
    pub fn empty_success() -> Self {
        Self::with_gas_used(0)
    }

    /// A successful receipt with the given `gas_used` at the latest supported FVM version.
    pub fn with_gas_used(gas_used: u64) -> Self {
        Receipt::V4(Receipt_v4 {
            exit_code: fvm_shared4::error::ExitCode::OK,
            return_data: Default::default(),
            gas_used,
            events_root: None,
        })
    }

    /// Writes `count` successful receipts to a fresh AMT and returns its root, in the same format
    /// [`Receipt::get_receipts`] reads.
    pub fn store_receipts(store: &impl Blockstore, count: usize) -> anyhow::Result<Cid> {
        Ok(Amtv0::new_from_iter(
            store,
            (0..count).map(|_| Receipt_v4 {
                exit_code: fvm_shared4::error::ExitCode::OK,
                return_data: Default::default(),
                gas_used: 0,
                events_root: None,
            }),
        )?)
    }
}

impl Entry {
    pub fn new(
        flags: crate::shim::fvm_shared_latest::event::Flags,
        key: String,
        codec: u64,
        value: Vec<u8>,
    ) -> Self {
        Entry::V4(Entry_v4 {
            flags,
            key,
            codec,
            value,
        })
    }
}

impl StampedEvent {
    /// A stamped event at the latest supported FVM version with a single `FLAG_INDEXED_ALL` entry
    /// whose key and value are `key`. Lets tests build events without naming a concrete FVM version.
    pub fn new_indexed(emitter: ActorID, key: &str) -> Self {
        Self::V4(create_raw_event_v4(emitter, key))
    }
}

/// Builds a raw FVM4 stamped event with a single indexed entry whose key and value are `key`.
/// Wrap in [`StampedEvent::V4`] for the shim-level type.
pub(crate) fn create_raw_event_v4(emitter: u64, key: &str) -> fvm_shared4::event::StampedEvent {
    fvm_shared4::event::StampedEvent {
        emitter,
        event: fvm_shared4::event::ActorEvent {
            entries: vec![fvm_shared4::event::Entry {
                flags: fvm_shared4::event::Flags::FLAG_INDEXED_ALL,
                key: key.to_string(),
                codec: fvm_ipld_encoding::IPLD_RAW,
                value: key.as_bytes().to_vec(),
            }],
        },
    }
}

/// Builds a raw FVM3 stamped event with a single indexed entry whose key and value are `key`.
/// Wrap in [`StampedEvent::V3`] for the shim-level type.
pub(crate) fn create_raw_event_v3(emitter: u64, key: &str) -> fvm_shared3::event::StampedEvent {
    fvm_shared3::event::StampedEvent {
        emitter,
        event: fvm_shared3::event::ActorEvent {
            entries: vec![fvm_shared3::event::Entry {
                flags: fvm_shared3::event::Flags::FLAG_INDEXED_ALL,
                key: key.to_string(),
                codec: fvm_ipld_encoding::IPLD_RAW,
                value: key.as_bytes().to_vec(),
            }],
        },
    }
}

impl quickcheck::Arbitrary for Receipt {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        #[derive(derive_quickcheck_arbitrary::Arbitrary, Clone)]
        enum Helper {
            V2 {
                exit_code: u32,
                return_data: Vec<u8>,
                gas_used: i64,
            },
            V3 {
                exit_code: u32,
                return_data: Vec<u8>,
                gas_used: u64,
                events_root: Option<::cid::Cid>,
            },
            V4 {
                exit_code: u32,
                return_data: Vec<u8>,
                gas_used: u64,
                events_root: Option<::cid::Cid>,
            },
        }
        match Helper::arbitrary(g) {
            Helper::V2 {
                exit_code,
                return_data,
                gas_used,
            } => Self::V2(Receipt_v2 {
                exit_code: exit_code.into(),
                return_data: return_data.into(),
                gas_used,
            }),
            Helper::V3 {
                exit_code,
                return_data,
                gas_used,
                events_root,
            } => Self::V3(Receipt_v3 {
                exit_code: exit_code.into(),
                return_data: return_data.into(),
                gas_used,
                events_root,
            }),
            Helper::V4 {
                exit_code,
                return_data,
                gas_used,
                events_root,
            } => Self::V4(Receipt_v4 {
                exit_code: exit_code.into(),
                return_data: return_data.into(),
                gas_used,
                events_root,
            }),
        }
    }
}

// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::trace::ExecutionEvent;
use crate::shim::{
    econ::TokenAmount, fvm_shared_latest::ActorID, fvm_shared_latest::error::ExitCode,
};
use crate::utils::get_size::{GetSize, vec_heap_size_with_fn_helper};
use cid::Cid;
use fil_actors_shared::fvm_ipld_amt::{Amt, Amtv0};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::RawBytes;
use fvm_shared2::receipt::Receipt as Receipt_v2;
use fvm_shared3::event::ActorEvent as ActorEvent_v3;
use fvm_shared3::event::Entry as Entry_v3;
use fvm_shared3::event::StampedEvent as StampedEvent_v3;
pub use fvm_shared3::receipt::Receipt as Receipt_v3;
use fvm_shared4::event::ActorEvent as ActorEvent_v4;
use fvm_shared4::event::Entry as Entry_v4;
use fvm_shared4::event::StampedEvent as StampedEvent_v4;
use fvm_shared4::receipt::Receipt as Receipt_v4;
use fvm2::executor::ApplyRet as ApplyRet_v2;
use fvm3::executor::ApplyRet as ApplyRet_v3;
use fvm4::executor::ApplyRet as ApplyRet_v4;
use serde::Serialize;
use spire_enum::prelude::delegated_enum;
use std::borrow::Borrow as _;

#[delegated_enum(impl_conversions)]
#[derive(Clone, Debug)]
pub enum ApplyRet {
    V2(ApplyRet_v2),
    V3(ApplyRet_v3),
    V4(ApplyRet_v4),
}

impl ApplyRet {
    pub fn failure_info(&self) -> Option<String> {
        delegate_apply_ret!(self => |r| r.failure_info.as_ref().map(|failure| format!("{failure} (RetCode={})", self.msg_receipt().exit_code())))
    }

    pub fn miner_tip(&self) -> TokenAmount {
        delegate_apply_ret!(self.miner_tip.borrow().into())
    }

    pub fn penalty(&self) -> TokenAmount {
        delegate_apply_ret!(self.penalty.borrow().into())
    }

    pub fn msg_receipt(&self) -> Receipt {
        delegate_apply_ret!(self.msg_receipt.clone().into())
    }

    pub fn refund(&self) -> TokenAmount {
        delegate_apply_ret!(self.refund.borrow().into())
    }

    pub fn base_fee_burn(&self) -> TokenAmount {
        delegate_apply_ret!(self.base_fee_burn.borrow().into())
    }

    pub fn over_estimation_burn(&self) -> TokenAmount {
        delegate_apply_ret!(self.over_estimation_burn.borrow().into())
    }

    pub fn exec_trace(&self) -> Vec<ExecutionEvent> {
        delegate_apply_ret!(self => |r| r.exec_trace.iter().cloned().map(Into::into).collect())
    }

    pub fn events(&self) -> Vec<StampedEvent> {
        match self {
            ApplyRet::V2(_) => Vec::<StampedEvent>::default(),
            ApplyRet::V3(v3) => v3.events.iter().cloned().map(Into::into).collect(),
            ApplyRet::V4(v4) => v4.events.iter().cloned().map(Into::into).collect(),
        }
    }
}

// Note: it's impossible to properly derive Deserialize.
// To deserialize into `Receipt`, refer to `fn get_parent_receipt`
#[delegated_enum(impl_conversions)]
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum Receipt {
    V2(Receipt_v2),
    V3(Receipt_v3),
    V4(Receipt_v4),
}

impl GetSize for Receipt {
    fn get_heap_size(&self) -> usize {
        delegate_receipt!(self.return_data.bytes().get_heap_size())
    }
}

impl PartialEq for Receipt {
    fn eq(&self, other: &Self) -> bool {
        self.exit_code() == other.exit_code()
            && self.return_data() == other.return_data()
            && self.gas_used() == other.gas_used()
            && self.events_root() == other.events_root()
    }
}

impl Receipt {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Receipt::V2(v2) => ExitCode::new(v2.exit_code.value()),
            Receipt::V3(v3) => ExitCode::new(v3.exit_code.value()),
            Receipt::V4(v4) => v4.exit_code,
        }
    }

    pub fn return_data(&self) -> RawBytes {
        delegate_receipt!(self.return_data.clone())
    }

    pub fn gas_used(&self) -> u64 {
        match self {
            Receipt::V2(v2) => v2.gas_used as u64,
            Receipt::V3(v3) => v3.gas_used,
            Receipt::V4(v4) => v4.gas_used,
        }
    }
    pub fn events_root(&self) -> Option<Cid> {
        match self {
            Receipt::V2(_) => None,
            Receipt::V3(v3) => v3.events_root,
            Receipt::V4(v4) => v4.events_root,
        }
    }

    pub fn get_receipt(
        db: &impl Blockstore,
        receipts: &Cid,
        i: u64,
    ) -> anyhow::Result<Option<Self>> {
        // Try Receipt_v4 first. (Receipt_v4 and Receipt_v3 are identical, use v4 here)
        if let Ok(amt) = Amtv0::load(receipts, db)
            && let Ok(receipts) = amt.get(i)
        {
            return Ok(receipts.cloned().map(Receipt::V4));
        }

        // Fallback to Receipt_v2.
        let amt = Amtv0::load(receipts, db)?;
        let receipts = amt.get(i)?;
        Ok(receipts.cloned().map(Receipt::V2))
    }

    pub fn get_receipts(db: &impl Blockstore, receipts_cid: Cid) -> anyhow::Result<Vec<Receipt>> {
        let mut receipts = Vec::new();

        // Try Receipt_v4 first. (Receipt_v4 and Receipt_v3 are identical, use v4 here)
        if let Ok(amt) = Amtv0::<fvm_shared4::receipt::Receipt, _>::load(&receipts_cid, db) {
            amt.for_each(|_, receipt| {
                receipts.push(Receipt::V4(receipt.clone()));
                Ok(())
            })?;
        } else {
            // Fallback to Receipt_v2.
            let amt = Amtv0::<fvm_shared2::receipt::Receipt, _>::load(&receipts_cid, db)?;
            amt.for_each(|_, receipt| {
                receipts.push(Receipt::V2(receipt.clone()));
                Ok(())
            })?;
        }

        Ok(receipts)
    }
}

#[delegated_enum(impl_conversions)]
#[derive(Clone, Debug)]
pub enum Entry {
    V3(Entry_v3),
    V4(Entry_v4),
}

impl Entry {
    #[cfg(test)]
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

    pub fn into_parts(self) -> (u64, String, u64, Vec<u8>) {
        delegate_entry!(self => |e| (e.flags.bits(), e.key, e.codec, e.value))
    }

    pub fn value(&self) -> &Vec<u8> {
        delegate_entry!(self.value.borrow())
    }

    pub fn codec(&self) -> u64 {
        delegate_entry!(self.codec)
    }

    pub fn key(&self) -> &String {
        delegate_entry!(self.key.borrow())
    }
}

#[delegated_enum(impl_conversions)]
#[derive(Clone, Debug)]
pub enum ActorEvent {
    V3(ActorEvent_v3),
    V4(ActorEvent_v4),
}

impl ActorEvent {
    pub fn entries(&self) -> Vec<Entry> {
        delegate_actor_event!(self => |e| e.entries.clone().into_iter().map(Into::into).collect())
    }
}

/// Event with extra information stamped by the FVM.
#[delegated_enum(impl_conversions)]
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum StampedEvent {
    V3(StampedEvent_v3),
    V4(StampedEvent_v4),
}

impl GetSize for StampedEvent {
    fn get_heap_size(&self) -> usize {
        delegate_stamped_event!(self => |e| vec_heap_size_with_fn_helper(&e.event.entries, |e| {
            e.key.get_heap_size() + e.value.get_heap_size()
        }))
    }
}

impl StampedEvent {
    /// Returns the ID of the actor that emitted this event.
    pub fn emitter(&self) -> ActorID {
        delegate_stamped_event!(self.emitter)
    }

    /// Returns the event as emitted by the actor.
    pub fn event(&self) -> ActorEvent {
        delegate_stamped_event!(self.event.clone().into())
    }

    /// Loads events directly from the events AMT root CID.
    /// Returns events in the exact order they are stored in the AMT.
    pub fn get_events<DB: Blockstore>(
        db: &DB,
        events_root: &Cid,
    ) -> anyhow::Result<Vec<StampedEvent>> {
        let mut events = Vec::new();

        // Try StampedEvent_v4 first (StampedEvent_v4 and StampedEvent_v3 are identical, use v4 here)
        if let Ok(amt) = Amt::<StampedEvent_v4, _>::load(events_root, db) {
            amt.for_each_cacheless(|_, event| {
                events.push(StampedEvent::V4(event.clone()));
                Ok(())
            })?;
        } else {
            // Fallback to StampedEvent_v3
            let amt = Amt::<StampedEvent_v3, _>::load(events_root, db)?;
            amt.for_each_cacheless(|_, event| {
                events.push(StampedEvent::V3(event.clone()));
                Ok(())
            })?;
        }

        Ok(events)
    }
}

#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn receipt_cbor_serde_serialize(receipt: Receipt) {
        let encoded = fvm_ipld_encoding::to_vec(&receipt).unwrap();
        let encoded2 = match &receipt {
            Receipt::V2(v) => fvm_ipld_encoding::to_vec(v),
            Receipt::V3(v) => fvm_ipld_encoding::to_vec(v),
            Receipt::V4(v) => fvm_ipld_encoding::to_vec(v),
        }
        .unwrap();
        assert_eq!(encoded, encoded2);
    }
}

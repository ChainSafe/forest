// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::trace::ExecutionEvent;
use crate::shim::{
    econ::TokenAmount, fvm_shared_latest::ActorID, fvm_shared_latest::error::ExitCode,
};
use cid::Cid;
use fil_actors_shared::fvm_ipld_amt::Amtv0;
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

#[derive(Clone, Debug)]
pub enum ApplyRet {
    V2(Box<ApplyRet_v2>),
    V3(Box<ApplyRet_v3>),
    V4(Box<ApplyRet_v4>),
}

impl From<ApplyRet_v2> for ApplyRet {
    fn from(other: ApplyRet_v2) -> Self {
        ApplyRet::V2(Box::new(other))
    }
}

impl From<ApplyRet_v3> for ApplyRet {
    fn from(other: ApplyRet_v3) -> Self {
        ApplyRet::V3(Box::new(other))
    }
}

impl From<ApplyRet_v4> for ApplyRet {
    fn from(other: ApplyRet_v4) -> Self {
        ApplyRet::V4(Box::new(other))
    }
}

impl ApplyRet {
    pub fn failure_info(&self) -> Option<String> {
        match self {
            ApplyRet::V2(v2) => v2.failure_info.as_ref().map(|failure| failure.to_string()),
            ApplyRet::V3(v3) => v3.failure_info.as_ref().map(|failure| failure.to_string()),
            ApplyRet::V4(v4) => v4.failure_info.as_ref().map(|failure| failure.to_string()),
        }
        .map(|e| format!("{} (RetCode={})", e, self.msg_receipt().exit_code()))
    }

    pub fn miner_tip(&self) -> TokenAmount {
        match self {
            ApplyRet::V2(v2) => (&v2.miner_tip).into(),
            ApplyRet::V3(v3) => (&v3.miner_tip).into(),
            ApplyRet::V4(v4) => (&v4.miner_tip).into(),
        }
    }

    pub fn penalty(&self) -> TokenAmount {
        match self {
            ApplyRet::V2(v2) => (&v2.penalty).into(),
            ApplyRet::V3(v3) => (&v3.penalty).into(),
            ApplyRet::V4(v4) => (&v4.penalty).into(),
        }
    }

    pub fn msg_receipt(&self) -> Receipt {
        match self {
            ApplyRet::V2(v2) => Receipt::V2(v2.msg_receipt.clone()),
            ApplyRet::V3(v3) => Receipt::V3(v3.msg_receipt.clone()),
            ApplyRet::V4(v4) => Receipt::V4(v4.msg_receipt.clone()),
        }
    }

    pub fn refund(&self) -> TokenAmount {
        match self {
            ApplyRet::V2(v2) => (&v2.refund).into(),
            ApplyRet::V3(v3) => (&v3.refund).into(),
            ApplyRet::V4(v4) => (&v4.refund).into(),
        }
    }

    pub fn base_fee_burn(&self) -> TokenAmount {
        match self {
            ApplyRet::V2(v2) => (&v2.base_fee_burn).into(),
            ApplyRet::V3(v3) => (&v3.base_fee_burn).into(),
            ApplyRet::V4(v4) => (&v4.base_fee_burn).into(),
        }
    }

    pub fn over_estimation_burn(&self) -> TokenAmount {
        match self {
            ApplyRet::V2(v2) => (&v2.over_estimation_burn).into(),
            ApplyRet::V3(v3) => (&v3.over_estimation_burn).into(),
            ApplyRet::V4(v4) => (&v4.over_estimation_burn).into(),
        }
    }

    pub fn exec_trace(&self) -> Vec<ExecutionEvent> {
        match self {
            ApplyRet::V2(v2) => v2.exec_trace.iter().cloned().map(Into::into).collect(),
            ApplyRet::V3(v3) => v3.exec_trace.iter().cloned().map(Into::into).collect(),
            ApplyRet::V4(v4) => v4.exec_trace.iter().cloned().map(Into::into).collect(),
        }
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
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum Receipt {
    V2(Receipt_v2),
    V3(Receipt_v3),
    V4(Receipt_v4),
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
        match self {
            Receipt::V2(v2) => v2.return_data.clone(),
            Receipt::V3(v3) => v3.return_data.clone(),
            Receipt::V4(v4) => v4.return_data.clone(),
        }
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

impl From<Receipt_v3> for Receipt {
    fn from(other: Receipt_v3) -> Self {
        Receipt::V3(other)
    }
}

#[derive(Clone, Debug)]
pub enum Entry {
    V3(Entry_v3),
    V4(Entry_v4),
}

impl From<Entry_v3> for Entry {
    fn from(other: Entry_v3) -> Self {
        Self::V3(other)
    }
}

impl From<Entry_v4> for Entry {
    fn from(other: Entry_v4) -> Self {
        Self::V4(other)
    }
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
        match self {
            Self::V3(v3) => {
                let Entry_v3 {
                    flags,
                    key,
                    codec,
                    value,
                } = v3;
                (flags.bits(), key, codec, value)
            }
            Self::V4(v4) => {
                let Entry_v4 {
                    flags,
                    key,
                    codec,
                    value,
                } = v4;
                (flags.bits(), key, codec, value)
            }
        }
    }

    pub fn value(&self) -> &Vec<u8> {
        match self {
            Self::V3(v3) => &v3.value,
            Self::V4(v4) => &v4.value,
        }
    }

    pub fn codec(&self) -> u64 {
        match self {
            Self::V3(v3) => v3.codec,
            Self::V4(v4) => v4.codec,
        }
    }

    pub fn key(&self) -> &String {
        match self {
            Self::V3(v3) => &v3.key,
            Self::V4(v4) => &v4.key,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ActorEvent {
    V3(ActorEvent_v3),
    V4(ActorEvent_v4),
}

impl From<ActorEvent_v3> for ActorEvent {
    fn from(other: ActorEvent_v3) -> Self {
        ActorEvent::V3(other)
    }
}

impl From<ActorEvent_v4> for ActorEvent {
    fn from(other: ActorEvent_v4) -> Self {
        ActorEvent::V4(other)
    }
}

impl ActorEvent {
    pub fn entries(&self) -> Vec<Entry> {
        match self {
            Self::V3(v3) => v3.entries.clone().into_iter().map(Into::into).collect(),
            Self::V4(v4) => v4.entries.clone().into_iter().map(Into::into).collect(),
        }
    }
}

/// Event with extra information stamped by the FVM.
#[derive(Clone, Debug, Serialize)]
pub enum StampedEvent {
    V3(StampedEvent_v3),
    V4(StampedEvent_v4),
}

impl From<StampedEvent_v3> for StampedEvent {
    fn from(other: StampedEvent_v3) -> Self {
        StampedEvent::V3(other)
    }
}

impl From<StampedEvent_v4> for StampedEvent {
    fn from(other: StampedEvent_v4) -> Self {
        StampedEvent::V4(other)
    }
}

impl StampedEvent {
    /// Returns the ID of the actor that emitted this event.
    pub fn emitter(&self) -> ActorID {
        match self {
            Self::V3(v3) => v3.emitter,
            Self::V4(v4) => v4.emitter,
        }
    }

    /// Returns the event as emitted by the actor.
    pub fn event(&self) -> ActorEvent {
        match self {
            Self::V3(v3) => v3.event.clone().into(),
            Self::V4(v4) => v4.event.clone().into(),
        }
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

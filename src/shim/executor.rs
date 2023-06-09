// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::borrow::Borrow;

use fvm::executor::ApplyRet as ApplyRet_v2;
use fvm3::executor::ApplyRet as ApplyRet_v3;
use fvm_ipld_encoding3::RawBytes;
use fvm_shared::receipt::Receipt as Receipt_v2;
use fvm_shared3::error::ExitCode;
pub use fvm_shared3::receipt::Receipt as Receipt_v3;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::shim::econ::TokenAmount;

#[derive(Clone, Debug)]
pub enum ApplyRet {
    V2(Box<ApplyRet_v2>),
    V3(Box<ApplyRet_v3>),
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

impl ApplyRet {
    pub fn failure_info(&self) -> Option<String> {
        match self {
            ApplyRet::V2(v2) => v2.failure_info.as_ref().map(|failure| failure.to_string()),
            ApplyRet::V3(v3) => v3.failure_info.as_ref().map(|failure| failure.to_string()),
        }
    }

    pub fn miner_tip(&self) -> TokenAmount {
        match self {
            ApplyRet::V2(v2) => v2.miner_tip.borrow().into(),
            ApplyRet::V3(v3) => v3.miner_tip.borrow().into(),
        }
    }

    pub fn penalty(&self) -> TokenAmount {
        match self {
            ApplyRet::V2(v2) => v2.penalty.borrow().into(),
            ApplyRet::V3(v3) => v3.penalty.borrow().into(),
        }
    }

    pub fn msg_receipt(&self) -> Receipt {
        match self {
            ApplyRet::V2(v2) => Receipt::V2(v2.msg_receipt.clone()),
            ApplyRet::V3(v3) => Receipt::V3(v3.msg_receipt.clone()),
        }
    }
}

#[derive(PartialEq, Clone, Debug)]
pub enum Receipt {
    V2(Receipt_v2),
    V3(Receipt_v3),
}

impl Serialize for Receipt {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Receipt::V2(v2) => v2.serialize(serializer),
            Receipt::V3(v3) => v3.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Receipt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Receipt_v2::deserialize(deserializer).map(Receipt::V2)
    }
}

impl Receipt {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Receipt::V2(v2) => ExitCode::new(v2.exit_code.value()),
            Receipt::V3(v3) => v3.exit_code,
        }
    }

    pub fn return_data(&self) -> RawBytes {
        match self {
            Receipt::V2(v2) => RawBytes::from(v2.return_data.to_vec()),
            Receipt::V3(v3) => v3.return_data.clone(),
        }
    }

    pub fn gas_used(&self) -> u64 {
        match self {
            Receipt::V2(v2) => v2.gas_used as u64,
            Receipt::V3(v3) => v3.gas_used,
        }
    }
}

impl From<Receipt_v3> for Receipt {
    fn from(other: Receipt_v3) -> Self {
        Receipt::V3(other)
    }
}

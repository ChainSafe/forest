// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{address::Address, clock::ChainEpoch};

pub enum ConsensusFaultType {
    DoubleForkMining,
    TimeOffsetMining,
    ParentGrinding,
}

impl From<ConsensusFaultType> for fvm_shared2::consensus::ConsensusFaultType {
    fn from(value: ConsensusFaultType) -> Self {
        match value {
            ConsensusFaultType::DoubleForkMining => Self::DoubleForkMining,
            ConsensusFaultType::TimeOffsetMining => Self::TimeOffsetMining,
            ConsensusFaultType::ParentGrinding => Self::ParentGrinding,
        }
    }
}

impl From<ConsensusFaultType> for fvm_shared3::consensus::ConsensusFaultType {
    fn from(value: ConsensusFaultType) -> Self {
        match value {
            ConsensusFaultType::DoubleForkMining => Self::DoubleForkMining,
            ConsensusFaultType::TimeOffsetMining => Self::TimeOffsetMining,
            ConsensusFaultType::ParentGrinding => Self::ParentGrinding,
        }
    }
}

impl From<ConsensusFaultType> for fvm_shared4::consensus::ConsensusFaultType {
    fn from(value: ConsensusFaultType) -> Self {
        match value {
            ConsensusFaultType::DoubleForkMining => Self::DoubleForkMining,
            ConsensusFaultType::TimeOffsetMining => Self::TimeOffsetMining,
            ConsensusFaultType::ParentGrinding => Self::ParentGrinding,
        }
    }
}

pub struct ConsensusFault {
    pub target: Address,
    pub epoch: ChainEpoch,
    pub fault_type: ConsensusFaultType,
}

impl From<ConsensusFault> for fvm_shared2::consensus::ConsensusFault {
    fn from(value: ConsensusFault) -> Self {
        Self {
            target: value.target.into(),
            epoch: value.epoch,
            fault_type: value.fault_type.into(),
        }
    }
}

impl From<ConsensusFault> for fvm_shared3::consensus::ConsensusFault {
    fn from(value: ConsensusFault) -> Self {
        Self {
            target: value.target.into(),
            epoch: value.epoch,
            fault_type: value.fault_type.into(),
        }
    }
}

impl From<ConsensusFault> for fvm_shared4::consensus::ConsensusFault {
    fn from(value: ConsensusFault) -> Self {
        Self {
            target: value.target.into(),
            epoch: value.epoch,
            fault_type: value.fault_type.into(),
        }
    }
}

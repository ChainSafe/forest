// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::CachingBlockHeader;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use serde::{Deserialize, Serialize};

/// Represents a detected consensus fault
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusFault {
    /// The miner address that committed the fault
    pub miner_address: Address,
    /// The epoch when the fault was detected
    pub detection_epoch: ChainEpoch,
    /// The type of consensus fault
    pub fault_type: ConsensusFaultType,
    /// The block headers involved in the fault
    pub block_header_1: CachingBlockHeader,
    /// The second block header involved in the fault
    pub block_header_2: CachingBlockHeader,
    /// Additional evidence for parent-grinding faults
    pub block_header_extra: Option<CachingBlockHeader>,
}

/// Types of consensus faults that can be detected
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash)]
pub enum ConsensusFaultType {
    /// Two blocks at the same epoch by the same miner
    DoubleForkMining,
    /// Two blocks with the same parents by the same miner
    TimeOffsetMining,
    /// Miner ignored their own block and mined on others
    ParentGrinding,
}

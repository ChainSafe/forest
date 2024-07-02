// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use fil_actor_interface::miner::Partition;

impl<'a> PartitionExt for Partition<'a> {
    fn terminated(&self) -> &BitField {
        match self {
            Partition::V8(dl) => &dl.terminated,
            Partition::V9(dl) => &dl.terminated,
            Partition::V10(dl) => &dl.terminated,
            Partition::V11(dl) => &dl.terminated,
            Partition::V12(dl) => &dl.terminated,
            Partition::V13(dl) => &dl.terminated,
            Partition::V14(dl) => &dl.terminated,
        }
    }

    fn expirations_epochs(&self) -> Cid {
        match self {
            Partition::V8(dl) => dl.expirations_epochs,
            Partition::V9(dl) => dl.expirations_epochs,
            Partition::V10(dl) => dl.expirations_epochs,
            Partition::V11(dl) => dl.expirations_epochs,
            Partition::V12(dl) => dl.expirations_epochs,
            Partition::V13(dl) => dl.expirations_epochs,
            Partition::V14(dl) => dl.expirations_epochs,
        }
    }
}

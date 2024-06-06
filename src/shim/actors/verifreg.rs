// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

use crate::shim::address::Address;
use ahash::HashMap;
use fil_actor_interface::verifreg::{Allocation, AllocationID, State};
use fvm_ipld_blockstore::Blockstore;

pub trait VerifiedRegistryStateExt {
    fn get_allocations<BS: Blockstore>(
        &self,
        store: &BS,
        address: &Address,
    ) -> anyhow::Result<HashMap<AllocationID, Allocation>>;
}

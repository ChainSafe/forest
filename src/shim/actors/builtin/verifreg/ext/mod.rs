// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

use crate::shim::actors::verifreg::{Allocation, AllocationID, Claim, State};
use crate::shim::address::Address;
use ahash::HashMap;
use fil_actor_verifreg_state::v13::ClaimID;
use fvm_ipld_blockstore::Blockstore;

pub trait VerifiedRegistryStateExt {
    fn get_allocations<BS: Blockstore>(
        &self,
        store: &BS,
        address: &Address,
    ) -> anyhow::Result<HashMap<AllocationID, Allocation>>;

    fn get_all_allocations<BS: Blockstore>(
        &self,
        store: &BS,
    ) -> anyhow::Result<HashMap<AllocationID, Allocation>>;

    fn get_claims<BS: Blockstore>(
        &self,
        store: &BS,
        provider_id_address: &Address,
    ) -> anyhow::Result<HashMap<ClaimID, Claim>>;

    fn get_all_claims<BS: Blockstore>(&self, store: &BS)
    -> anyhow::Result<HashMap<ClaimID, Claim>>;
}

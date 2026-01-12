// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod balance_table;
mod state;

use crate::shim::actors::{market, verifreg::AllocationID};
use crate::shim::address::Address;
use crate::shim::deal::DealID;
use crate::shim::econ::TokenAmount;
use ahash::HashMap;
use fvm_ipld_blockstore::Blockstore;

pub trait MarketStateExt {
    fn get_allocations_for_pending_deals(
        &self,
        store: &impl Blockstore,
    ) -> anyhow::Result<HashMap<DealID, AllocationID>>;

    fn get_allocation_id_for_pending_deal(
        &self,
        store: &impl Blockstore,
        deal_id: &DealID,
    ) -> anyhow::Result<AllocationID>;
}

pub trait BalanceTableExt {
    fn for_each<F>(&self, f: F) -> anyhow::Result<()>
    where
        F: FnMut(&Address, &TokenAmount) -> anyhow::Result<()>;
}

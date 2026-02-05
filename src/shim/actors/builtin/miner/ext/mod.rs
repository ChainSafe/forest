// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod deadline;
mod state;

use crate::rpc::types::SectorOnChainInfo;
use crate::shim::{
    actors::miner::{Deadline, DeadlineInfo, State},
    clock::ChainEpoch,
    econ::TokenAmount,
    runtime::Policy,
};
use crate::utils::db::CborStoreExt as _;
use cid::Cid;
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fvm_ipld_blockstore::Blockstore;

pub trait MinerStateExt {
    /// Loads sectors corresponding to the bitfield. If no bitfield is passed
    /// in, return all.
    fn load_sectors_ext<BS: Blockstore>(
        &self,
        store: &BS,
        sectors: Option<&BitField>,
    ) -> anyhow::Result<Vec<SectorOnChainInfo>>;
}

pub trait DeadlineExt {
    fn daily_fee(&self) -> TokenAmount;
}

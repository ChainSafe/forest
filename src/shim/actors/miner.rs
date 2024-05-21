// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

use fil_actor_interface::miner::State;
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fvm_ipld_blockstore::Blockstore;

use crate::rpc::types::SectorOnChainInfo;

pub trait MinerStateExt {
    /// Loads sectors corresponding to the bitfield. If no bitfield is passed
    /// in, return all.
    fn load_sectors_ext<BS: Blockstore>(
        &self,
        store: &BS,
        sectors: Option<&BitField>,
    ) -> anyhow::Result<Vec<SectorOnChainInfo>>;
}

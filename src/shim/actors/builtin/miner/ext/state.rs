// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::super::load_sectors_by_version;
use super::*;

impl MinerStateExt for State {
    fn load_sectors_ext<BS: Blockstore>(
        &self,
        store: &BS,
        sectors: Option<&BitField>,
    ) -> anyhow::Result<Vec<SectorOnChainInfo>> {
        load_sectors_by_version!(self, store, sectors; 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19)
    }
}

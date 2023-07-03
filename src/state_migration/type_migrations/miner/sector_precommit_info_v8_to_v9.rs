// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actor_miner_state::{
    v8::SectorPreCommitInfo as SectorPreCommitInfoV8,
    v9::{CompactCommD as CompactCommDV9, SectorPreCommitInfo as SectorPreCommitInfoV9},
};
use fvm_ipld_blockstore::Blockstore;

use super::super::super::common::{TypeMigration, TypeMigrator};

impl TypeMigration<SectorPreCommitInfoV8, SectorPreCommitInfoV9> for TypeMigrator {
    fn migrate_type(
        from: SectorPreCommitInfoV8,
        _: &impl Blockstore,
    ) -> anyhow::Result<SectorPreCommitInfoV9> {
        let out_info = SectorPreCommitInfoV9 {
            seal_proof: from.seal_proof,
            sector_number: from.sector_number,
            sealed_cid: from.sealed_cid,
            seal_rand_epoch: from.seal_rand_epoch,
            deal_ids: from.deal_ids,
            expiration: from.expiration,
            unsealed_cid: CompactCommDV9::default(),
        };

        Ok(out_info)
    }
}

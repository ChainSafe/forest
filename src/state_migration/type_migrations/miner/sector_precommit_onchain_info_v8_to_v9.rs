// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actor_miner_state::{
    v8::SectorPreCommitOnChainInfo as SectorPreCommitOnChainInfoV8,
    v9::SectorPreCommitOnChainInfo as SectorPreCommitOnChainInfoV9,
};
use fvm_ipld_blockstore::Blockstore;

use super::super::super::common::{TypeMigration, TypeMigrator};

impl TypeMigration<SectorPreCommitOnChainInfoV8, SectorPreCommitOnChainInfoV9> for TypeMigrator {
    fn migrate_type(
        from: SectorPreCommitOnChainInfoV8,
        store: &impl Blockstore,
    ) -> anyhow::Result<SectorPreCommitOnChainInfoV9> {
        let out_info = SectorPreCommitOnChainInfoV9 {
            pre_commit_deposit: from.pre_commit_deposit,
            pre_commit_epoch: from.pre_commit_epoch,
            info: TypeMigrator::migrate_type(from.info, store)?,
        };

        Ok(out_info)
    }
}

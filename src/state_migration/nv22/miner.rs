// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV22` upgrade for the
//! Miner actor. While the `NV22` upgrade does not change the state of the
//! Miner actor, it does change the state of the Market actor, which requires
//! metadata from the Miner actor.
//!
//! As per [FIP-0076](https://github.com/filecoin-project/FIPs/blob/master/FIPS/fip-0076.md#backwards-compatibility)
//! > This proposal requires a state migration to the market actor to add the new `ProviderSectors` mapping,
//! > and to add a sector number to and remove allocation ID from each `DealState`. Computing this mapping
//! > requires reading all sector metadata from the miner actor.

use crate::state_migration::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};
use crate::utils::db::CborStoreExt as _;
use ahash::HashMap;
use cid::Cid;
use fil_actor_miner_state::v12::State as MinerStateOld;
use fil_actors_shared::v12::Array as ArrayOld;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared4::ActorID;
use fvm_shared4::clock::ChainEpoch;
use fvm_shared4::deal::DealID;
use fvm_shared4::sector::{SectorID, SectorNumber};
use parking_lot::RwLock;
use std::sync::Arc;

#[derive(Default)]
pub struct ProviderSectors {
    pub deal_to_sector: RwLock<HashMap<DealID, SectorID>>,
    pub miner_to_sector_to_deals: RwLock<HashMap<ActorID, HashMap<SectorNumber, Vec<DealID>>>>,
}

pub struct MinerMigrator {
    upgrade_epoch: ChainEpoch,
    provider_sectors: Arc<ProviderSectors>,
    out_cid: Cid,
}

pub(in crate::state_migration) fn miner_migrator<BS: Blockstore>(
    upgrade_epoch: ChainEpoch,
    provider_sectors: Arc<ProviderSectors>,
    out_cid: Cid,
) -> anyhow::Result<Arc<dyn ActorMigration<BS> + Send + Sync>> {
    Ok(Arc::new(MinerMigrator {
        upgrade_epoch,
        provider_sectors,
        out_cid,
    }))
}

impl<BS: Blockstore> ActorMigration<BS> for MinerMigrator {
    fn migrate_state(
        &self,
        store: &BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<Option<ActorMigrationOutput>> {
        let miner_id = input.address.id()?;
        let in_state: MinerStateOld = store.get_cbor_required(&input.head)?;

        let in_sectors = ArrayOld::<fil_actor_miner_state::v12::SectorOnChainInfo, BS>::load(
            &in_state.sectors,
            store,
        )?;

        in_sectors.for_each(|i, sector| {
            if sector.deal_ids.is_empty() || sector.expiration < self.upgrade_epoch {
                return Ok(());
            }

            let mut sectors = self.provider_sectors.deal_to_sector.write();
            for deal_id in sector.deal_ids.iter() {
                sectors.insert(
                    *deal_id,
                    SectorID {
                        miner: miner_id,
                        number: i,
                    },
                );
            }
            drop(sectors);

            let mut sector_deals = self.provider_sectors.miner_to_sector_to_deals.write();
            sector_deals
                .entry(miner_id)
                .or_default()
                .insert(i, sector.deal_ids.clone());

            Ok(())
        })?;

        Ok(Some(ActorMigrationOutput {
            new_code_cid: self.out_cid,
            new_head: input.head,
        }))
    }
}

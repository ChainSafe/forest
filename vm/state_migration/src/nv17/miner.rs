// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV17` upgrade for the miner
//! actor.

use std::sync::Arc;

use cid::{multihash::Code::Blake2b256, Cid};
use fil_actor_miner_state::{v8::State as MinerStateOld, v9::State as MinerStateNew};
use forest_shim::{piece::piece_v2, sector::SectorNumber};
use forest_utils::db::CborStoreExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use fvm_ipld_hamt::BytesKey;

use crate::common::{
    ActorMigration, ActorMigrationInput, ActorMigrationOutput, TypeMigration, TypeMigrator,
};

pub struct MinerMigrator {
    out_code: Cid,
    market_proposals: Cid,
    empty_precommit_map_cid_v9: Cid,
    empty_deadline_v8_cid: Cid,
    empty_deadline_v9_cid: Cid,
    empty_deadlines_v9_cid: Cid,
}

pub(crate) fn miner_migrator<BS>(
    out_code: Cid,
    store: &BS,
    market_proposals: Cid,
) -> anyhow::Result<Arc<dyn ActorMigration<BS> + Send + Sync>>
where
    BS: Blockstore + Clone + Send + Sync,
{
    let mut empty_precommit_map = fil_actors_shared::v9::make_empty_map::<_, Cid>(
        store,
        fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH,
    );
    let empty_precommit_map_cid_v9 = empty_precommit_map.flush()?;

    let empty_deadline_v8: fil_actor_miner_state::v8::Deadline =
        fil_actor_miner_state::v8::Deadline::new(store)?;
    let empty_deadline_v8_cid = store.put_cbor(&empty_deadline_v8, Blake2b256)?;

    let empty_deadline_v9 = fil_actor_miner_state::v9::Deadline::new(store)?;
    let empty_deadline_v9_cid = store.put_cbor(&empty_deadline_v9, Blake2b256)?;

    // FIXME: pass policy from chain config
    let policy = fil_actors_shared::v9::runtime::Policy::calibnet();
    let empty_deadlines_v9 =
        fil_actor_miner_state::v9::Deadlines::new(&policy, empty_deadline_v9_cid);
    let empty_deadlines_v9_cid = store.put_cbor(&empty_deadlines_v9, Blake2b256)?;

    Ok(Arc::new(MinerMigrator {
        out_code,
        market_proposals,
        empty_precommit_map_cid_v9,
        empty_deadline_v8_cid,
        empty_deadline_v9_cid,
        empty_deadlines_v9_cid,
    }))
}

impl<BS> ActorMigration<BS> for MinerMigrator
where
    BS: Blockstore + Clone + Send + Sync,
{
    fn migrate_state(
        &self,
        store: BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<ActorMigrationOutput> {
        let in_state: MinerStateOld = store
            .get_cbor(&input.head)?
            .ok_or_else(|| anyhow::anyhow!("Init actor: could not read v9 state"))?;
        let new_pre_committed_sectors =
            self.migrate_pre_committed_sectors(&store, &in_state.pre_committed_sectors)?;
        let new_sectors = self.migrate_sectors(&store, &in_state.sectors)?;
        let new_deadlines = self.migrate_deadlines(&store, &in_state.deadlines)?;

        let mut out_state: MinerStateNew = TypeMigrator::migrate_type(in_state, &store)?;
        out_state.pre_committed_sectors = new_pre_committed_sectors;
        out_state.sectors = new_sectors;
        out_state.deadlines = new_deadlines;

        let new_head = store.put_cbor_default(&out_state)?;

        Ok(ActorMigrationOutput {
            new_code_cid: self.out_code,
            new_head,
        })
    }
}

impl MinerMigrator {
    fn migrate_pre_committed_sectors(
        &self,
        store: &impl Blockstore,
        old_pre_committed_sectors: &Cid,
    ) -> anyhow::Result<Cid> {
        const HAMT_BIT_WIDTH: u32 = fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH;

        // FIXME: `DEFAULT_BIT_WIDTH` on rust side is 3 while it's 5 on go side. Revisit to make sure
        // it does not effect `load` API here. (Go API takes bit_width=5 for loading while Rust API does not)
        //
        // P.S. Because of lifetime limitation, this is not stored as a field of `MinerMigrator` like in Go code
        let market_proposals = fil_actors_shared::v8::Array::<
            fil_actor_market_state::v8::DealProposal,
            _,
        >::load(&self.market_proposals, &store)?;

        let old_precommit_on_chain_infos =
            fil_actors_shared::v8::make_map_with_root_and_bitwidth::<
                _,
                fil_actor_miner_state::v8::SectorPreCommitOnChainInfo,
            >(old_pre_committed_sectors, store, HAMT_BIT_WIDTH)?;

        let mut new_precommit_on_chain_infos = fil_actors_shared::v9::make_empty_map::<
            _,
            fil_actor_miner_state::v9::SectorPreCommitOnChainInfo,
        >(store, HAMT_BIT_WIDTH);

        old_precommit_on_chain_infos.for_each(|key, value| {
            let mut pieces = vec![];
            for &deal_id in &value.info.deal_ids {
                let deal = market_proposals.get(deal_id)?;
                // Continue on not found to match Go logic
                //
                // Possible for the proposal to be missing if it's expired (but the deal is still in a precommit that's yet to be cleaned up)
                // Just continue in this case, the sector is unProveCommitable anyway, will just fail later
                if let Some(deal) = deal {
                    pieces.push(piece_v2::PieceInfo {
                        cid: deal.piece_cid,
                        size: deal.piece_size,
                    });
                }
            }

            let unsealed_cid = if !pieces.is_empty() {
                Some(fvm_sdk::crypto::compute_unsealed_sector_cid(
                    value.info.seal_proof,
                    pieces.as_slice(),
                )?)
            } else{
                None
            };

            let mut sector_precommit_onchain_info:fil_actor_miner_state::v9::SectorPreCommitOnChainInfo = TypeMigrator::migrate_type(value.clone(), store)?;
            sector_precommit_onchain_info.info.unsealed_cid = fil_actor_miner_state::v9::CompactCommD(unsealed_cid);
            new_precommit_on_chain_infos.set(sector_key(value.info.sector_number)?, sector_precommit_onchain_info)?;
            Ok(())
        })?;

        Ok(new_precommit_on_chain_infos.flush()?)
    }

    fn migrate_sectors(
        &self,
        store: &impl Blockstore,
        // old_cache: &Cid,
        // old_address: &Cid,
        old_sectors: &Cid,
    ) -> anyhow::Result<Cid> {
        todo!()
    }

    fn migrate_deadlines(
        &self,
        store: &impl Blockstore,
        // old_cache: &Cid,
        // old_address: &Cid,
        old_deadlines: &Cid,
    ) -> anyhow::Result<Cid> {
        todo!()
    }
}

// TODO: Replace with <https://github.com/ChainSafe/fil-actor-states/pull/125>
fn sector_key(sector: SectorNumber) -> anyhow::Result<BytesKey> {
    let mut buffer = unsigned_varint::encode::u64_buffer();
    Ok(unsigned_varint::encode::u64(sector, &mut buffer)
        .to_vec()
        .into())
}

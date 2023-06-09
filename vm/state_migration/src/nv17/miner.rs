// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV17` upgrade for the miner
//! actor.

use std::sync::Arc;

use ahash::HashMap;
use anyhow::Context;
use cid::{multibase::Base, multihash::Code::Blake2b256, Cid};
use fil_actor_miner_state::{
    v8::State as MinerStateOld,
    v9::{util::sector_key, State as MinerStateNew},
};
use fil_actors_shared::abi::commp::compute_unsealed_sector_cid_v2;
use forest_shim::{address::Address, piece::piece_v2};
use forest_utils::db::CborStoreExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;

use crate::common::{
    ActorMigration, ActorMigrationInput, ActorMigrationOutput, TypeMigration, TypeMigrator,
};

pub struct MinerMigrator {
    out_code: Cid,
    market_proposals: Cid,
    empty_precommit_map_cid_v9: Cid,
    empty_deadline_v8_cid: Cid,
    empty_deadlines_v8_cid: Cid,
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
    use fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH;

    let mut empty_precommit_map =
        fil_actors_shared::v9::make_empty_map::<_, Cid>(store, HAMT_BIT_WIDTH);
    let empty_precommit_map_cid_v9 = empty_precommit_map.flush()?;

    let empty_deadline_v8: fil_actor_miner_state::v8::Deadline =
        fil_actor_miner_state::v8::Deadline::new(store)?;
    let empty_deadline_v8_cid = store.put_cbor_default(&empty_deadline_v8)?;

    // FIXME: pass policy from chain config
    let policy = fil_actors_shared::v8::runtime::Policy::calibnet();
    let empty_deadlines_v8 =
        fil_actor_miner_state::v8::Deadlines::new(&policy, empty_deadline_v8_cid);
    let empty_deadlines_v8_cid = store.put_cbor_default(&empty_deadlines_v8)?;

    let empty_deadline_v9 = fil_actor_miner_state::v9::Deadline::new(store)?;
    let empty_deadline_v9_cid = store.put_cbor_default(&empty_deadline_v9)?;

    // FIXME: pass policy from chain config
    let policy = fil_actors_shared::v9::runtime::Policy::calibnet();
    let empty_deadlines_v9 =
        fil_actor_miner_state::v9::Deadlines::new(&policy, empty_deadline_v9_cid);
    let empty_deadlines_v9_cid = store.put_cbor_default(&empty_deadlines_v9)?;

    Ok(Arc::new(MinerMigrator {
        out_code,
        market_proposals,
        empty_precommit_map_cid_v9,
        empty_deadline_v8_cid,
        empty_deadlines_v8_cid,
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
        let mut cache: HashMap<String, Cid> = Default::default();
        let in_state: MinerStateOld = store
            .get_cbor(&input.head)?
            .ok_or_else(|| anyhow::anyhow!("Init actor: could not read v9 state"))?;
        let new_pre_committed_sectors =
            self.migrate_pre_committed_sectors(&store, &in_state.pre_committed_sectors)?;
        let new_sectors =
            self.migrate_sectors_with_cache(&mut cache, &store, &input.address, &in_state.sectors)?;
        let new_deadlines = self.migrate_deadlines(&mut cache, &store, &in_state.deadlines)?;

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
        use fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH;

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
                Some(compute_unsealed_sector_cid_v2(
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

    fn migrate_sectors_with_cache(
        &self,
        cache: &mut HashMap<String, Cid>,
        store: &impl Blockstore,
        miner_address: &Address,
        in_root: &Cid,
    ) -> anyhow::Result<Cid> {
        let key = sectors_amt_key(in_root)?;

        if let Some(v) = cache.get(&key) {
            Ok(*v)
        } else {
            let in_array = fil_actors_shared::v8::Array::<
                fil_actor_miner_state::v8::SectorOnChainInfo,
                _,
            >::load(in_root, store)?;

            let prev_in_root = cache.get(&miner_prev_sectors_in_key(miner_address));
            let prev_out_root = cache.get(&miner_prev_sectors_out_key(miner_address));

            if let Some(prev_in_root) = prev_in_root {
                if let Some(prev_out_root) = prev_out_root {
                    // we have previous work, but the AMT has changed -- diff them
                }
            }

            todo!()
        }
    }

    fn migrate_deadlines(
        &self,
        cache: &mut HashMap<String, Cid>,
        store: &impl Blockstore,
        deadlines: &Cid,
    ) -> anyhow::Result<Cid> {
        if deadlines == &self.empty_deadlines_v8_cid {
            Ok(self.empty_deadline_v9_cid.clone())
        } else {
            let in_deadlines: fil_actor_miner_state::v8::Deadlines = store
                .get_cbor(deadlines)?
                .context("Failed to get in_deadlines")?;

            // FIXME: pass policy from chain config
            let policy = fil_actors_shared::v9::runtime::Policy::calibnet();
            let mut out_deadlines =
                fil_actor_miner_state::v9::Deadlines::new(&policy, self.empty_deadline_v9_cid);
            for (i, c) in in_deadlines.due.iter().enumerate() {
                if c == &self.empty_deadline_v8_cid {
                    if i < out_deadlines.due.len() {
                        out_deadlines.due[i] = *c;
                    } else {
                        out_deadlines.due.push(*c);
                    }
                } else {
                    let in_deadline: fil_actor_miner_state::v8::Deadline =
                        store.get_cbor(c)?.context("Failed to get in_deadline")?;

                    let out_sectors_snapshot_cid_cache_key =
                        sectors_amt_key(&in_deadline.sectors_snapshot)?;
                    let out_sectors_snapshot_cid =
                        match cache.get(&out_sectors_snapshot_cid_cache_key) {
                            Some(v) => v,
                            None => {
                                todo!()
                            }
                        };

                    let out_deadline = fil_actor_miner_state::v9::Deadline {
                        partitions: in_deadline.partitions,
                        expirations_epochs: in_deadline.expirations_epochs,
                        partitions_posted: in_deadline.partitions_posted,
                        early_terminations: in_deadline.early_terminations,
                        live_sectors: in_deadline.live_sectors,
                        total_sectors: in_deadline.total_sectors,
                        faulty_power: TypeMigrator::migrate_type(in_deadline.faulty_power, store)?,
                        optimistic_post_submissions: in_deadline.optimistic_post_submissions,
                        sectors_snapshot: *out_sectors_snapshot_cid,
                        partitions_snapshot: in_deadline.partitions_snapshot,
                        optimistic_post_submissions_snapshot: in_deadline
                            .optimistic_post_submissions_snapshot,
                    };

                    let out_deadline_cid = store.put_cbor_default(&out_deadline)?;

                    if i < out_deadlines.due.len() {
                        out_deadlines.due[i] = out_deadline_cid;
                    } else {
                        out_deadlines.due.push(out_deadline_cid);
                    }
                }
            }

            store.put_cbor_default(&out_deadlines)
        }
    }
}

fn migrate_from_scratch<'bs, BS: Blockstore>(
    store: &'bs BS,
    in_array: &fil_actors_shared::v8::Array<fil_actor_miner_state::v8::SectorOnChainInfo, BS>,
) -> anyhow::Result<
    fil_actors_shared::v9::Array<'bs, fil_actor_miner_state::v9::SectorOnChainInfo, BS>,
> {
    use fil_actor_miner_state::v9::SECTORS_AMT_BITWIDTH;

    let mut out_array = fil_actors_shared::v9::Array::<
        fil_actor_miner_state::v9::SectorOnChainInfo,
        _,
    >::new_with_bit_width(store, SECTORS_AMT_BITWIDTH);

    in_array.for_each(|key, info_v8| {
        out_array.set(key, TypeMigrator::migrate_type(info_v8.clone(), store)?)?;
        Ok(())
    })?;

    Ok(out_array)
}

fn miner_prev_sectors_in_key(addr: &Address) -> String {
    format!("prevSectorsIn-{addr}")
}

fn miner_prev_sectors_out_key(addr: &Address) -> String {
    format!("prevSectorsOut-{addr}")
}

fn sectors_amt_key(cid: &Cid) -> anyhow::Result<String> {
    Ok(format!(
        "sectorsAmt-{}",
        cid.to_string_of_base(Base::Base32Lower)?,
    ))
}

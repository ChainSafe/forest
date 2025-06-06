// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV17` upgrade for the miner
//! actor.

use std::sync::Arc;

use crate::networks::NetworkChain;
use crate::shim::{address::Address, piece::PieceInfo};
use crate::utils::db::CborStoreExt;
use crate::{make_calibnet_policy, make_mainnet_policy};
use ahash::HashMap;
use anyhow::Context as _;
use cid::{Cid, multibase::Base};
use fil_actor_miner_state::{
    v8::State as MinerStateOld,
    v9::{State as MinerStateNew, util::sector_key},
};
use fil_actors_shared::abi::commp::compute_unsealed_sector_cid_v2;
use fil_actors_shared::fvm_ipld_amt;
use fvm_ipld_blockstore::Blockstore;

use super::super::common::{
    ActorMigration, ActorMigrationInput, ActorMigrationOutput, TypeMigration, TypeMigrator,
};

pub struct MinerMigrator {
    chain: NetworkChain,
    out_code: Cid,
    market_proposals: Cid,
    empty_deadline_v8_cid: Cid,
    empty_deadlines_v8_cid: Cid,
    empty_deadline_v9_cid: Cid,
    empty_deadlines_v9_cid: Cid,
}

pub(super) fn miner_migrator<BS>(
    out_code: Cid,
    store: &Arc<BS>,
    market_proposals: Cid,
    chain: NetworkChain,
) -> anyhow::Result<Arc<dyn ActorMigration<BS> + Send + Sync>>
where
    BS: Blockstore + Send + Sync,
{
    let empty_deadline_v8: fil_actor_miner_state::v8::Deadline =
        fil_actor_miner_state::v8::Deadline::new(store)?;
    let empty_deadline_v8_cid = store.put_cbor_default(&empty_deadline_v8)?;

    let policy = match chain {
        NetworkChain::Mainnet => make_mainnet_policy!(v8),
        NetworkChain::Calibnet => make_calibnet_policy!(v8),
        NetworkChain::Devnet(_) => unimplemented!("Policy::devnet"),
        NetworkChain::Butterflynet => unimplemented!("Policy::butterflynet"),
    };
    let empty_deadlines_v8 =
        fil_actor_miner_state::v8::Deadlines::new(&policy, empty_deadline_v8_cid);
    let empty_deadlines_v8_cid = store.put_cbor_default(&empty_deadlines_v8)?;

    let empty_deadline_v9 = fil_actor_miner_state::v9::Deadline::new(store)?;
    let empty_deadline_v9_cid = store.put_cbor_default(&empty_deadline_v9)?;

    let policy = match chain {
        NetworkChain::Mainnet => make_mainnet_policy!(v9),
        NetworkChain::Calibnet => make_calibnet_policy!(v9),
        NetworkChain::Devnet(_) => unimplemented!("Policy::devnet"),
        NetworkChain::Butterflynet => unimplemented!("Policy::butterflynet"),
    };
    let empty_deadlines_v9 =
        fil_actor_miner_state::v9::Deadlines::new(&policy, empty_deadline_v9_cid);
    let empty_deadlines_v9_cid = store.put_cbor_default(&empty_deadlines_v9)?;

    Ok(Arc::new(MinerMigrator {
        chain,
        out_code,
        market_proposals,
        empty_deadline_v8_cid,
        empty_deadlines_v8_cid,
        empty_deadline_v9_cid,
        empty_deadlines_v9_cid,
    }))
}

impl<BS> ActorMigration<BS> for MinerMigrator
where
    BS: Blockstore + Send + Sync,
{
    fn migrate_state(
        &self,
        store: &BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<Option<ActorMigrationOutput>> {
        let mut cache: HashMap<String, Cid> = Default::default();

        let in_state: MinerStateOld = store.get_cbor_required(&input.head)?;
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

        Ok(Some(ActorMigrationOutput {
            new_code_cid: self.out_code,
            new_head,
        }))
    }
}

impl MinerMigrator {
    fn migrate_pre_committed_sectors(
        &self,
        store: &impl Blockstore,
        old_pre_committed_sectors: &Cid,
    ) -> anyhow::Result<Cid> {
        use fil_actors_shared::v9::builtin::HAMT_BIT_WIDTH;

        // Because of lifetime limitation, this is not stored as a field of `MinerMigrator` like in Go code
        let market_proposals = fil_actors_shared::v8::Array::<
            fil_actor_market_state::v8::DealProposal,
            _,
        >::load(&self.market_proposals, store)?;

        let old_precommit_on_chain_infos =
            fil_actors_shared::v8::make_map_with_root_and_bitwidth::<
                _,
                fil_actor_miner_state::v8::SectorPreCommitOnChainInfo,
            >(old_pre_committed_sectors, store, HAMT_BIT_WIDTH)?;

        let mut new_precommit_on_chain_infos = fil_actors_shared::v9::make_empty_map::<
            _,
            fil_actor_miner_state::v9::SectorPreCommitOnChainInfo,
        >(store, HAMT_BIT_WIDTH);

        old_precommit_on_chain_infos.for_each(|_key, value| {
            let mut pieces = vec![];
            for &deal_id in &value.info.deal_ids {
                let deal = market_proposals.get(deal_id)?;
                // Continue on not found to match Go logic
                //
                // Possible for the proposal to be missing if it's expired (but the deal is still in a precommit that's yet to be cleaned up)
                // Just continue in this case, the sector is unProveCommitable anyway, will just fail later
                if let Some(deal) = deal {
                    pieces.push(PieceInfo::new(deal.piece_cid,deal.piece_size.into()).into());
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

            let mut out_array = if let Some(prev_in_root) = prev_in_root {
                if let Some(prev_out_root) = prev_out_root {
                    // we have previous work, but the AMT has changed -- diff them
                    let prev_in_sectors = fvm_ipld_amt::Amt::<
                        fil_actor_miner_state::v8::SectorOnChainInfo,
                        _,
                    >::load(prev_in_root, store)?;
                    let in_sectors = fvm_ipld_amt::Amt::<
                        fil_actor_miner_state::v8::SectorOnChainInfo,
                        _,
                    >::load(in_root, store)?;
                    let changes = fvm_ipld_amt::diff(&prev_in_sectors, &in_sectors)?;
                    let mut prev_out_sectors = fil_actors_shared::v9::Array::<
                        fil_actor_miner_state::v9::SectorOnChainInfo,
                        _,
                    >::load(prev_out_root, store)?;
                    for change in changes {
                        use fvm_ipld_amt::ChangeType;
                        match &change.change_type() {
                            ChangeType::Remove => {
                                prev_out_sectors.delete(change.key)?;
                            }
                            ChangeType::Modify | ChangeType::Add => {
                                let info_v8 = in_sectors
                                    .get(change.key)?
                                    .context("Failed to get info from in_sectors")?;
                                prev_out_sectors.set(
                                    change.key,
                                    TypeMigrator::migrate_type(info_v8.clone(), store)?,
                                )?;
                            }
                        };
                    }
                    prev_out_sectors
                } else {
                    migrate_from_scratch(store, &in_array)?
                }
            } else {
                migrate_from_scratch(store, &in_array)?
            };

            let out_root = out_array.flush()?;
            cache.insert(miner_prev_sectors_in_key(miner_address), *in_root);
            cache.insert(miner_prev_sectors_out_key(miner_address), out_root);

            cache.insert(key, out_root);

            Ok(out_root)
        }
    }

    fn migrate_deadlines(
        &self,
        cache: &mut HashMap<String, Cid>,
        store: &impl Blockstore,
        deadlines: &Cid,
    ) -> anyhow::Result<Cid> {
        if deadlines == &self.empty_deadlines_v8_cid {
            Ok(self.empty_deadlines_v9_cid)
        } else {
            let in_deadlines: fil_actor_miner_state::v8::Deadlines =
                store.get_cbor_required(deadlines)?;

            let policy = match &self.chain {
                NetworkChain::Mainnet => make_mainnet_policy!(v9),
                NetworkChain::Calibnet => make_calibnet_policy!(v9),
                NetworkChain::Devnet(_) => unimplemented!("Policy::devnet"),
                NetworkChain::Butterflynet => unimplemented!("Policy::butterflynet"),
            };
            let mut out_deadlines =
                fil_actor_miner_state::v9::Deadlines::new(&policy, self.empty_deadline_v9_cid);
            for (i, c) in in_deadlines.due.iter().enumerate() {
                if c == &self.empty_deadline_v8_cid {
                    if let Some(due_i) = out_deadlines.due.get_mut(i) {
                        *due_i = *c;
                    } else {
                        out_deadlines.due.push(*c);
                    }
                } else {
                    let in_deadline: fil_actor_miner_state::v8::Deadline =
                        store.get_cbor_required(c)?;

                    let out_sectors_snapshot_cid_cache_key =
                        sectors_amt_key(&in_deadline.sectors_snapshot)?;
                    let out_sectors_snapshot_cid =
                        match cache.get(&out_sectors_snapshot_cid_cache_key) {
                            Some(v) => *v,
                            None => {
                                let in_sectors_snapshot = fil_actors_shared::v8::Array::load(
                                    &in_deadline.sectors_snapshot,
                                    store,
                                )?;
                                let mut out_sectors_snapshot =
                                    migrate_from_scratch(store, &in_sectors_snapshot)?;
                                let out_sectors_snapshot_cid = out_sectors_snapshot.flush()?;
                                cache.insert(
                                    out_sectors_snapshot_cid_cache_key,
                                    out_sectors_snapshot_cid,
                                );
                                out_sectors_snapshot_cid
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
                        sectors_snapshot: out_sectors_snapshot_cid,
                        partitions_snapshot: in_deadline.partitions_snapshot,
                        optimistic_post_submissions_snapshot: in_deadline
                            .optimistic_post_submissions_snapshot,
                    };

                    let out_deadline_cid = store.put_cbor_default(&out_deadline)?;

                    if let Some(due_i) = out_deadlines.due.get_mut(i) {
                        *due_i = out_deadline_cid;
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
    format!("prev_sectors_in_{addr}")
}

fn miner_prev_sectors_out_key(addr: &Address) -> String {
    format!("prev_sectors_out_{addr}")
}

fn sectors_amt_key(cid: &Cid) -> anyhow::Result<String> {
    Ok(format!(
        "sectors_amt_{}",
        cid.to_string_of_base(Base::Base32Lower)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::networks::{ChainConfig, Height};
    use crate::shim::actors::BURNT_FUNDS_ACTOR_ADDR;
    use crate::shim::actors::*;
    use crate::shim::bigint::BigInt;
    use crate::shim::{
        econ::TokenAmount,
        machine::{BuiltinActor, BuiltinActorManifest},
        state_tree::{ActorState, StateRoot, StateTree, StateTreeVersion},
    };
    use crate::utils::multihash::prelude::*;
    use fil_actors_shared::fvm_ipld_hamt::BytesKey;
    use fvm_ipld_encoding::IPLD_RAW;
    use fvm_shared2::{
        bigint::Zero,
        commcid::{
            FIL_COMMITMENT_SEALED, FIL_COMMITMENT_UNSEALED, POSEIDON_BLS12_381_A1_FC1,
            SHA2_256_TRUNC254_PADDED,
        },
        piece::PaddedPieceSize,
    };
    use multihash_codetable::Multihash;

    #[test]
    fn test_nv17_miner_migration() {
        let store = Arc::new(crate::db::MemoryDB::default());
        let (mut state_tree_old, manifest_old) = make_input_tree(&store);
        let system_actor_old = state_tree_old
            .get_actor(&system::ADDRESS.into())
            .unwrap()
            .unwrap();
        let system_state_old: fil_actor_system_state::v9::State =
            store.get_cbor_required(&system_actor_old.state).unwrap();
        let manifest_data_cid_old = system_state_old.builtin_actors;
        assert_eq!(manifest_data_cid_old, manifest_old.source_cid());
        assert_eq!(
            manifest_data_cid_old.to_string(),
            "bafy2bzaceb7wfqkjc5c3ccjyhaf7zuhkvbzpvhnb35feaettztoharc7zdndc"
        );

        let base_addr_id = 10000;
        let base_addr = Address::new_id(base_addr_id);
        let base_worker_addr = Address::new_id(base_addr_id + 100);

        // create 3 deal proposals
        let mut market_actor_old = state_tree_old
            .get_actor(&market::ADDRESS.into())
            .unwrap()
            .unwrap();
        let mut market_state_old: fil_actor_market_state::v8::State =
            store.get_cbor_required(&market_actor_old.state).unwrap();
        let mut proposals = fil_actors_shared::v8::Array::<
            fil_actor_market_state::v8::DealProposal,
            _,
        >::load(&market_state_old.proposals, &store)
        .unwrap();
        let base_deal = fil_actor_market_state::v8::DealProposal {
            piece_cid: Default::default(),
            piece_size: PaddedPieceSize(512),
            verified_deal: true,
            client: base_addr.into(),
            provider: base_addr.into(),
            label: fil_actor_market_state::v8::Label::String("".into()),
            start_epoch: 0,
            end_epoch: 0,
            storage_price_per_epoch: Zero::zero(),
            provider_collateral: Zero::zero(),
            client_collateral: Zero::zero(),
        };
        let deal0 = {
            let mut deal = base_deal.clone();
            deal.piece_cid = make_piece_cid("0".as_bytes());
            assert_eq!(
                deal.piece_cid.to_string(),
                "baga6ea4seaqf73hlm374q3zy3fjhq3dnnfwhtqw3yi452turwrtstvz2e75vp2i"
            );
            deal
        };
        let deal1 = {
            let mut deal = base_deal.clone();
            deal.piece_cid = make_piece_cid("1".as_bytes());
            assert_eq!(
                deal.piece_cid.to_string(),
                "baga6ea4seaqgxbvsop7tj7hbtvvyatx7li7vor5nutvkely5jhab4uw5w6dvwsy"
            );
            deal
        };
        let deal2 = {
            let mut deal = base_deal;
            deal.piece_cid = make_piece_cid("2".as_bytes());
            assert_eq!(
                deal.piece_cid.to_string(),
                "baga6ea4seaqni426hitf4fxo4a7vs4mltnoqgam4a7mlnri7sdnduzto5qj2wni"
            );
            deal
        };

        let mut pending_proposals =
            fil_actors_shared::v8::Set::from_root(&store, &market_state_old.pending_proposals)
                .unwrap();

        proposals.set(0, deal0).unwrap();
        pending_proposals
            .put(BytesKey(deal1.cid().unwrap().to_bytes()))
            .unwrap();
        proposals.set(1, deal1).unwrap();
        pending_proposals
            .put(BytesKey(deal2.cid().unwrap().to_bytes()))
            .unwrap();
        proposals.set(2, deal2).unwrap();

        market_state_old.proposals = proposals.flush().unwrap();
        assert_eq!(
            market_state_old.proposals.to_string(),
            "bafy2bzacecskt5brcfawiowjlv5lwnskkks2xzsmsnhkmjixndqlxuyb3abxs"
        );
        market_state_old.pending_proposals = pending_proposals.root().unwrap();

        let market_state_cid_old = store.put_cbor_default(&market_state_old).unwrap();
        market_actor_old.state = market_state_cid_old;
        state_tree_old
            .set_actor(&market::ADDRESS.into(), market_actor_old)
            .unwrap();

        // base stuff to create miners
        let miner_cid_old = manifest_old.get(BuiltinActor::Miner).unwrap();
        assert_eq!(
            miner_cid_old.to_string(),
            "bafkqaetgnfwc6obpon2g64tbm5sw22lomvza"
        );
        let base_miner_state = make_base_miner_state(&store, &base_addr, &base_worker_addr);

        let base_precommit = fil_actor_miner_state::v8::SectorPreCommitOnChainInfo {
            pre_commit_deposit: Zero::zero(),
            pre_commit_epoch: 0,
            deal_weight: Zero::zero(),
            verified_deal_weight: Zero::zero(),
            info: fil_actor_miner_state::v8::SectorPreCommitInfo {
                seal_proof: fvm_shared2::sector::RegisteredSealProof::StackedDRG32GiBV1P1,
                sealed_cid: make_sealed_cid("100".as_bytes()),
                ..Default::default()
            },
        };
        assert_eq!(
            base_precommit.info.sealed_cid.to_string(),
            "bagboea4b5abcblkxgzugketokvsj5szdvyourcdvislw57venjeowxmfu3xljuyg"
        );

        // make 3 miners
        // miner1 has no precommits at all
        // miner2 has 4 precommits, but with no deals
        // miner3 has 3 precommits, with deals [0], [1,2], and []

        // miner1 has no precommits at all
        let miner1_state_cid = store.put_cbor_default(&base_miner_state).unwrap();
        assert_eq!(
            miner1_state_cid.to_string(),
            "bafy2bzaceaqtktd7f5b2gutreh3b2czp2mkqu4ilyuu7mjcpwrk75g5nl6w6k"
        );

        let miner1 = ActorState::new(miner_cid_old, miner1_state_cid, Zero::zero(), 0, None);
        let addr1 = Address::new_id(base_addr_id + 1);
        state_tree_old.set_actor(&addr1, miner1).unwrap();

        // miner2 has precommits, but they have no deals
        let mut precommits2 = fil_actors_shared::v8::make_map_with_root::<
            _,
            fil_actor_miner_state::v8::SectorPreCommitOnChainInfo,
        >(&base_miner_state.pre_committed_sectors, &store)
        .unwrap();
        precommits2
            .set(sector_key(0).unwrap(), base_precommit.clone())
            .unwrap();
        precommits2
            .set(sector_key(1).unwrap(), base_precommit.clone())
            .unwrap();
        precommits2
            .set(sector_key(2).unwrap(), base_precommit.clone())
            .unwrap();
        precommits2
            .set(sector_key(3).unwrap(), base_precommit.clone())
            .unwrap();

        let precommit2_cid = precommits2.flush().unwrap();
        assert_eq!(
            precommit2_cid.to_string(),
            "bafy2bzacedogkdulyeaujgdsiqzo323s5dpz44efwihxsuekkkpo4znpl3g2s"
        );

        let mut miner2_state = base_miner_state.clone();
        miner2_state.pre_committed_sectors = precommit2_cid;
        let miner2_state_cid = store.put_cbor_default(&miner2_state).unwrap();
        assert_eq!(
            miner2_state_cid.to_string(),
            "bafy2bzacedad6xkymehkuoij4rhg2inzqnfin3er52znw53lddn5364usp2bi"
        );

        let miner2 = ActorState::new(miner_cid_old, miner2_state_cid, Zero::zero(), 0, None);
        let addr2 = Address::new_id(base_addr_id + 2);
        state_tree_old.set_actor(&addr2, miner2).unwrap();

        // miner 3 has precommits, some of which have deals
        let mut precommits3 = fil_actors_shared::v8::make_map_with_root::<
            _,
            fil_actor_miner_state::v8::SectorPreCommitOnChainInfo,
        >(&base_miner_state.pre_committed_sectors, &store)
        .unwrap();
        let mut precommits3dot0 = base_precommit.clone();
        precommits3dot0.info.deal_ids = vec![0];
        precommits3dot0.info.sector_number = 0;

        let mut precommits3dot1 = base_precommit.clone();
        precommits3dot1.info.deal_ids = vec![1, 2];
        precommits3dot1.info.sector_number = 1;

        let mut precommits3dot2 = base_precommit;
        precommits3dot2.info.sector_number = 2;

        precommits3
            .set(sector_key(0).unwrap(), precommits3dot0)
            .unwrap();
        precommits3
            .set(sector_key(1).unwrap(), precommits3dot1)
            .unwrap();
        precommits3
            .set(sector_key(2).unwrap(), precommits3dot2)
            .unwrap();

        let precommits3_cid = precommits3.flush().unwrap();
        assert_eq!(
            precommits3_cid.to_string(),
            "bafy2bzacecdpddgu5sxniq5iez3xapyxvi3dg7pc5oxthywuclvxyj4h2vweo"
        );

        let mut miner3_state = base_miner_state.clone();
        miner3_state.pre_committed_sectors = precommits3_cid;
        let miner3_state_cid = store.put_cbor_default(&miner3_state).unwrap();
        assert_eq!(
            miner3_state_cid.to_string(),
            "bafy2bzaceb7ojujla7jb6iaxeuk4etg2kui4gtjujwqadqkc7lkp4ugoqrh2m"
        );

        let miner3 = ActorState::new(miner_cid_old, miner3_state_cid, Zero::zero(), 0, None);
        let addr3 = Address::new_id(base_addr_id + 3);
        state_tree_old.set_actor(&addr3, miner3).unwrap();

        let tree_root = state_tree_old.flush().unwrap();

        let (new_manifest_cid, _new_manifest) = make_test_manifest(&store, "fil/9/");

        let mut chain_config = ChainConfig::calibnet();
        if let Some(entry) = chain_config.height_infos.get_mut(&Height::Shark) {
            entry.bundle = Some(new_manifest_cid);
        }
        let new_state_cid =
            super::super::run_migration(&chain_config, &store, &tree_root, 200).unwrap();
        let actors_out_state_root: StateRoot = store.get_cbor_required(&new_state_cid).unwrap();
        assert_eq!(
            actors_out_state_root.actors.to_string(),
            "bafy2bzacedgtk3lnnyfxnzc32etqaj3zvi7ar7nxq2jtxd2qr36ftbsjoycqu"
        );
        let new_state_cid2 =
            super::super::run_migration(&chain_config, &store, &tree_root, 200).unwrap();
        assert_eq!(new_state_cid, new_state_cid2);
    }

    #[test]
    fn test_fip0029_miner_migration() {
        let store = Arc::new(crate::db::MemoryDB::default());
        let (mut state_tree_old, manifest_old) = make_input_tree(&store);
        let addr = Address::new_id(10000);
        let worker_addr = Address::new_id(20000);
        let miner_cid_old = manifest_old.get(BuiltinActor::Miner).unwrap();
        let miner_state = make_base_miner_state(&store, &addr, &worker_addr);
        let miner_state_cid = store.put_cbor_default(&miner_state).unwrap();
        assert_eq!(
            miner_state_cid.to_string(),
            "bafy2bzaceacitm72b4zks7ivplygpmyqa6aas2pxkv4rkiknluxiko5mn4ng6"
        );
        let miner_actor = ActorState::new(miner_cid_old, miner_state_cid, Zero::zero(), 0, None);
        state_tree_old.set_actor(&addr, miner_actor).unwrap();
        let state_tree_old_root = state_tree_old.flush().unwrap();
        let (new_manifest_cid, _new_manifest) = make_test_manifest(&store, "fil/9/");
        let mut chain_config = ChainConfig::calibnet();
        if let Some(entry) = chain_config.height_infos.get_mut(&Height::Shark) {
            entry.bundle = Some(new_manifest_cid);
        }
        let new_state_cid =
            super::super::run_migration(&chain_config, &store, &state_tree_old_root, 200).unwrap();
        let actors_out_state_root: StateRoot = store.get_cbor_required(&new_state_cid).unwrap();
        assert_eq!(
            actors_out_state_root.actors.to_string(),
            "bafy2bzacebdpnjjyspbyj7al7d6234kdhkmdygkfdkp6zyao5o3egsfmribty"
        );
    }

    fn make_input_tree<BS: Blockstore>(store: &Arc<BS>) -> (StateTree<BS>, BuiltinActorManifest) {
        let mut tree = StateTree::new(store.clone(), StateTreeVersion::V4).unwrap();

        let (_manifest_cid, manifest) = make_test_manifest(&store, "fil/8/");
        let account_cid = manifest.get(BuiltinActor::Account).unwrap();
        // fmt.Printf("accountCid: %s\n", accountCid)
        assert_eq!(account_cid.to_string(), "bafkqadlgnfwc6obpmfrwg33vnz2a");
        let system_cid = manifest.get_system();
        // fmt.Printf("systemCid: %s\n", systemCid)
        assert_eq!(system_cid.to_string(), "bafkqaddgnfwc6obpon4xg5dfnu");
        let system_state = fil_actor_system_state::v9::State {
            builtin_actors: manifest.source_cid(),
        };
        let system_state_cid = store.put_cbor_default(&system_state).unwrap();
        assert_eq!(
            system_state_cid.to_string(),
            "bafy2bzacebrujchvrqxwiml3aaud4ts7kgj74kkf7qewwmrsj5tvhneeamtlq"
        );
        init_actor(
            &mut tree,
            system_state_cid,
            system_cid,
            &system::ADDRESS.into(),
            Zero::zero(),
        );

        let init_cid = manifest.get_init();
        // fmt.Printf("initCid: %s\n", initCid)
        assert_eq!(init_cid.to_string(), "bafkqactgnfwc6obpnfxgs5a");
        let init_state =
            fil_actor_init_state::v8::State::new(&store, "migrationtest".into()).unwrap();
        let init_state_cid = store.put_cbor_default(&init_state).unwrap();
        assert_eq!(
            init_state_cid.to_string(),
            "bafy2bzacednf3o3eyjwkm2isixe5lezt6klncgz5axriewegbkw34r6pqszbe"
        );
        init_actor(
            &mut tree,
            init_state_cid,
            init_cid,
            &init::ADDRESS.into(),
            Zero::zero(),
        );

        let reward_cid = manifest.get(BuiltinActor::Reward).unwrap();
        assert_eq!(reward_cid.to_string(), "bafkqaddgnfwc6obpojsxoylsmq");
        let reward_state = fil_actor_reward_state::v8::State::new(Default::default());
        let reward_state_cid = store.put_cbor_default(&reward_state).unwrap();
        assert_eq!(
            reward_state_cid.to_string(),
            "bafy2bzaceaslbmsgmgmfi6pn2osvqcfuqinauuyt67zifnefurhpk4zxd2fos"
        );
        init_actor(
            &mut tree,
            reward_state_cid,
            reward_cid,
            &reward::ADDRESS.into(),
            TokenAmount::from_whole(1_100_000_000),
        );

        let cron_cid = manifest.get(BuiltinActor::Cron).unwrap();
        assert_eq!(cron_cid.to_string(), "bafkqactgnfwc6obpmnzg63q");
        let cron_state = fil_actor_cron_state::v8::State {
            entries: vec![
                fil_actor_cron_state::v8::Entry {
                    receiver: crate::shim::actors::power::ADDRESS,
                    method_num: crate::shim::actors::power::Method::OnEpochTickEnd as u64,
                },
                fil_actor_cron_state::v8::Entry {
                    receiver: crate::shim::actors::market::ADDRESS,
                    method_num: crate::shim::actors::market::Method::CronTick as u64,
                },
            ],
        };
        let cron_state_cid = store.put_cbor_default(&cron_state).unwrap();
        assert_eq!(
            cron_state_cid.to_string(),
            "bafy2bzacebs5dwwxmsjmzvoqcamx3qtl2x5qpqgpqxgnzl7scccmbvd37ulvs"
        );
        init_actor(
            &mut tree,
            cron_state_cid,
            cron_cid,
            &cron::ADDRESS.into(),
            Zero::zero(),
        );

        let power_cid = manifest.get(BuiltinActor::Power).unwrap();
        assert_eq!(
            power_cid.to_string(),
            "bafkqaetgnfwc6obpon2g64tbm5sxa33xmvza"
        );
        let power_state = fil_actor_power_state::v8::State::new(&store).unwrap();
        let power_state_cid = store.put_cbor_default(&power_state).unwrap();
        assert_eq!(
            power_state_cid.to_string(),
            "bafy2bzacebx3h3ib435qrzwb7zj7enrgepqeiyyeqpq6zwygasoag4m3mhy3w"
        );
        init_actor(
            &mut tree,
            power_state_cid,
            power_cid,
            &power::ADDRESS.into(),
            Zero::zero(),
        );

        let market_cid = manifest.get(BuiltinActor::Market).unwrap();
        assert_eq!(
            market_cid.to_string(),
            "bafkqae3gnfwc6obpon2g64tbm5sw2ylsnnsxi"
        );
        let market_state = fil_actor_market_state::v8::State::new(&store).unwrap();
        let market_state_cid = store.put_cbor_default(&market_state).unwrap();
        assert_eq!(
            market_state_cid.to_string(),
            "bafy2bzacea5udmevoj4io3yqy7ku7aitblugdvirbirg7wstzstb5xub5empc"
        );
        init_actor(
            &mut tree,
            market_state_cid,
            market_cid,
            &market::ADDRESS.into(),
            Zero::zero(),
        );

        // this will need to be replaced with the address of a multisig actor for the verified registry to be tested accurately
        let verifreg_root = Address::new_id(80);
        let account_state = fil_actor_account_state::v8::State {
            address: verifreg_root.into(),
        };
        let account_state_cid = store.put_cbor_default(&account_state).unwrap();
        assert_eq!(
            account_state_cid.to_string(),
            "bafy2bzaceajm42pledpxusdh4owdrdfvv463dthqg24npeeaz4jlbgzdcgkve"
        );
        init_actor(
            &mut tree,
            account_state_cid,
            account_cid,
            &account_state.address.into(),
            Zero::zero(),
        );

        let verifreg_cid = manifest.get(BuiltinActor::VerifiedRegistry).unwrap();
        assert_eq!(
            verifreg_cid.to_string(),
            "bafkqaftgnfwc6obpozsxe2lgnfswi4tfm5uxg5dspe"
        );
        let mut verifreg_state =
            fil_actor_verifreg_state::v8::State::new(&store, verifreg_root.into()).unwrap();
        let mut verified_clients = fil_actors_shared::v8::make_empty_map::<BS, BigInt>(
            store,
            fil_actors_shared::v8::builtin::HAMT_BIT_WIDTH,
        );
        // verified_clients is not set in the original go tests
        //
        // ```go
        // verifiedClients, _ := adt8.MakeEmptyMap(store, 5)
        // tmpAddr, _ := address.NewIDAddress(1001)
        // tmpAddrKey := abi.AddrKey(tmpAddr)
        // one := big.NewInt(1)
        // _ = verifiedClients.Put(tmpAddrKey, &one)
        // tmpAddr, _ = address.NewIDAddress(1002)
        // two := big.NewInt(2)
        // _ = verifiedClients.Put(abi.AddrKey(tmpAddr), &two)
        // verifiedClientsCID, _ := verifiedClients.Root()
        // vrState.VerifiedClients = verifiedClientsCID
        // ```
        {
            verified_clients
                .set(
                    BytesKey(Address::new_id(1001).to_bytes()),
                    num_bigint::BigInt::from(1).into(),
                )
                .unwrap();
            verified_clients
                .set(
                    BytesKey(Address::new_id(1002).to_bytes()),
                    num_bigint::BigInt::from(2).into(),
                )
                .unwrap();
            verifreg_state.verified_clients = verified_clients.flush().unwrap();
        }
        let verifreg_state_cid = store.put_cbor_default(&verifreg_state).unwrap();
        init_actor(
            &mut tree,
            verifreg_state_cid,
            verifreg_cid,
            &fil_actors_shared::v8::builtin::VERIFIED_REGISTRY_ACTOR_ADDR.into(),
            Zero::zero(),
        );

        // burnt funds
        let account_state = fil_actor_account_state::v8::State {
            address: BURNT_FUNDS_ACTOR_ADDR,
        };
        let account_state_cid = store.put_cbor_default(&account_state).unwrap();
        assert_eq!(
            account_state_cid.to_string(),
            "bafy2bzacedpuk5ggwoq3s2wixsyjjnexpsjstdlyntio76vs2lt2jvy3o6mau"
        );
        init_actor(
            &mut tree,
            account_state_cid,
            account_cid,
            &account_state.address.into(),
            Zero::zero(),
        );

        tree.flush().unwrap();

        (tree, manifest)
    }

    fn init_actor<BS: Blockstore>(
        tree: &mut StateTree<BS>,
        state: Cid,
        code: Cid,
        addr: &Address,
        balance: TokenAmount,
    ) {
        let actor = ActorState::new(code, state, balance, 0, None);
        tree.set_actor(addr, actor).unwrap();
    }

    fn make_test_manifest<BS: Blockstore>(store: &BS, prefix: &str) -> (Cid, BuiltinActorManifest) {
        let mut manifest_data = vec![];
        for name in [
            "account",
            "cron",
            "init",
            "storagemarket",
            "storageminer",
            "multisig",
            "paymentchannel",
            "storagepower",
            "reward",
            "system",
            "verifiedregistry",
            "datacap",
        ] {
            let hash = MultihashCode::Identity.digest(format!("{prefix}{name}").as_bytes());
            let code_cid = Cid::new_v1(IPLD_RAW, hash);
            manifest_data.push((name, code_cid));
        }

        let manifest_cid = store
            .put_cbor_default(&(1, store.put_cbor_default(&manifest_data).unwrap()))
            .unwrap();
        let manifest = BuiltinActorManifest::load_manifest(store, &manifest_cid).unwrap();

        (manifest_cid, manifest)
    }

    fn make_base_miner_state<BS: Blockstore>(
        store: &BS,
        base_addr: &Address,
        base_worker_addr: &Address,
    ) -> fil_actor_miner_state::v8::State {
        let empty_miner_info = fil_actor_miner_state::v8::MinerInfo {
            owner: base_addr.into(),
            worker: base_worker_addr.into(),
            control_addresses: vec![],
            pending_worker_key: None,
            peer_id: vec![],
            multi_address: vec![],
            window_post_proof_type: fvm_shared2::sector::RegisteredPoStProof::Invalid(0),
            sector_size: fvm_shared2::sector::SectorSize::_2KiB, // 0 not available in rust API, change Go code to 2 << 10 and all tests still pass
            window_post_partition_sectors: 0,
            consensus_fault_elapsed: 0,
            pending_owner_address: None,
        };

        let empty_miner_info_cid = store.put_cbor_default(&empty_miner_info).unwrap();

        fil_actor_miner_state::v8::State::new(
            &make_calibnet_policy!(v8),
            store,
            empty_miner_info_cid,
            0,
            0,
        )
        .unwrap()
    }

    fn make_piece_cid(data: &[u8]) -> Cid {
        let hash = MultihashCode::Sha2_256.digest(data);
        let hash = Multihash::wrap(SHA2_256_TRUNC254_PADDED, hash.digest()).unwrap();
        Cid::new_v1(FIL_COMMITMENT_UNSEALED, hash)
    }

    fn make_sealed_cid(data: &[u8]) -> Cid {
        let hash = MultihashCode::Sha2_256.digest(data);
        let hash = Multihash::wrap(POSEIDON_BLS12_381_A1_FC1, hash.digest()).unwrap();
        Cid::new_v1(FIL_COMMITMENT_SEALED, hash)
    }
}

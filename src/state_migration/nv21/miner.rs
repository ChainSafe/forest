// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV21` upgrade for the
//! Miner actor.

use std::sync::Arc;

use crate::shim::econ::TokenAmount;
use crate::{
    shim::address::Address, state_migration::common::MigrationCache, utils::db::CborStoreExt,
};
use anyhow::Context;
use cid::{multibase::Base, Cid};
use fil_actor_miner_state::{
    v11::Deadline as DeadlineOld, v11::Deadlines as DeadlinesOld, v11::State as MinerStateOld,
    v12::Deadline as DeadlineNew, v12::Deadlines as DeadlinesNew, v12::State as MinerStateNew,
};
use fil_actors_shared::fvm_ipld_amt;
use fil_actors_shared::v11::{runtime::Policy as PolicyOld, Array as ArrayOld};
use fil_actors_shared::v12::{runtime::Policy as PolicyNew, Array as ArrayNew};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;

use crate::state_migration::common::{
    ActorMigration, ActorMigrationInput, ActorMigrationOutput, TypeMigration, TypeMigrator,
};

pub struct MinerMigrator {
    empty_deadline_v11: Cid,
    empty_deadlines_v11: Cid,
    empty_deadline_v12: Cid,
    empty_deadlines_v12: Cid,
    policy_new: PolicyNew,
    out_cid: Cid,
}

pub(in crate::state_migration) fn miner_migrator<BS: Blockstore>(
    policy_old: &PolicyOld,
    policy_new: &PolicyNew,
    store: &Arc<BS>,
    out_cid: Cid,
) -> anyhow::Result<Arc<dyn ActorMigration<BS> + Send + Sync>> {
    let empty_deadline_v11 = DeadlineOld::new(store)?;
    let empty_deadline_v11 = store.put_cbor_default(&empty_deadline_v11)?;

    let empty_deadlines_v11 = DeadlinesOld::new(policy_old, empty_deadline_v11);
    let empty_deadlines_v11 = store.put_cbor_default(&empty_deadlines_v11)?;

    let empty_deadline_v12 = DeadlineNew::new(store)?;
    let empty_deadline_v12 = store.put_cbor_default(&empty_deadline_v12)?;

    let empty_deadlines_v12 = DeadlinesNew::new(policy_new, empty_deadline_v12);
    let empty_deadlines_v12 = store.put_cbor_default(&empty_deadlines_v12)?;

    Ok(Arc::new(MinerMigrator {
        empty_deadline_v11,
        empty_deadlines_v11,
        empty_deadline_v12,
        empty_deadlines_v12,
        policy_new: policy_new.clone(),
        out_cid,
    }))
}

impl<BS: Blockstore> ActorMigration<BS> for MinerMigrator {
    fn migrate_state(
        &self,
        store: &BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<Option<ActorMigrationOutput>> {
        let in_state: MinerStateOld = store
            .get_cbor(&input.head)?
            .context("Miner actor: could not read v11 state")?;

        let new_sectors = self.migrate_sectors_with_cache(
            store,
            &input.cache,
            &input.address,
            &in_state.sectors,
        )?;

        let new_deadlines =
            self.migrate_deadlines(store, &input.cache, &input.address, &in_state.deadlines)?;

        let out_state = MinerStateNew {
            info: in_state.info,
            pre_commit_deposits: TokenAmount::from(in_state.pre_commit_deposits).into(),
            locked_funds: TokenAmount::from(in_state.locked_funds).into(),
            vesting_funds: in_state.vesting_funds,
            fee_debt: TokenAmount::from(in_state.fee_debt).into(),
            initial_pledge: TokenAmount::from(in_state.initial_pledge).into(),
            pre_committed_sectors: in_state.pre_committed_sectors,
            pre_committed_sectors_cleanup: in_state.pre_committed_sectors_cleanup,
            allocated_sectors: in_state.allocated_sectors,
            sectors: new_sectors,
            proving_period_start: in_state.proving_period_start,
            current_deadline: in_state.current_deadline,
            deadlines: new_deadlines,
            early_terminations: in_state.early_terminations,
            deadline_cron_active: in_state.deadline_cron_active,
        };
        let new_head = store.put_cbor_default(&out_state)?;

        Ok(Some(ActorMigrationOutput {
            new_code_cid: self.out_cid,
            new_head,
        }))
    }
}

impl MinerMigrator {
    fn migrate_sectors_with_cache<BS: Blockstore>(
        &self,
        store: &BS,
        cache: &MigrationCache,
        address: &Address,
        in_root: &Cid,
    ) -> anyhow::Result<Cid> {
        cache
            .get_or_insert_with(sectors_amt_key(in_root)?, || -> anyhow::Result<Cid> {
                let prev_in = cache.get(&miner_prev_sectors_in_key(address));
                let prev_out = cache.get(&miner_prev_sectors_out_key(address));

                let out_root = if let (Some(prev_in), Some(prev_out)) = (prev_in, prev_out) {
                    self.migrate_sectors_with_diff(store, in_root, &prev_in, &prev_out)?
                } else {
                    let in_array = ArrayOld::load(in_root, store)?;
                    let mut out_array = self.migrate_sectors_from_scratch(store, &in_array)?;
                    out_array.flush()?
                };

                cache.insert(miner_prev_sectors_in_key(address), in_root.to_owned());
                cache.insert(miner_prev_sectors_out_key(address), out_root.to_owned());
                Ok(out_root)
            })
            .map(|cid| cid.to_owned())
    }

    fn migrate_sectors_with_diff<BS: Blockstore>(
        &self,
        store: &BS,
        in_root: &Cid,
        prev_in: &Cid,
        prev_out: &Cid,
    ) -> anyhow::Result<Cid> {
        let prev_in_sectors =
            ArrayOld::<fil_actor_miner_state::v11::SectorOnChainInfo, BS>::load(prev_in, store)?;
        let in_sectors =
            ArrayOld::<fil_actor_miner_state::v11::SectorOnChainInfo, BS>::load(in_root, store)?;

        let diffs = fvm_ipld_amt::diff(&prev_in_sectors, &in_sectors)?;

        let mut prev_out_sectors =
            ArrayNew::<fil_actor_miner_state::v12::SectorOnChainInfo, BS>::load(prev_out, store)?;

        for diff in diffs {
            use fvm_ipld_amt::ChangeType;
            match &diff.change_type() {
                ChangeType::Remove => {
                    prev_out_sectors.delete(diff.key)?;
                }
                ChangeType::Modify | ChangeType::Add => {
                    let info = in_sectors
                        .get(diff.key)?
                        .context("Failed to get info from in_sectors")?;
                    prev_out_sectors
                        .set(diff.key, TypeMigrator::migrate_type(info.clone(), store)?)?;
                }
            };
        }

        Ok(prev_out_sectors.flush()?)
    }

    fn migrate_sectors_from_scratch<'bs, BS: Blockstore>(
        &self,
        store: &'bs BS,
        in_array: &ArrayOld<fil_actor_miner_state::v11::SectorOnChainInfo, BS>,
    ) -> anyhow::Result<ArrayNew<'bs, fil_actor_miner_state::v12::SectorOnChainInfo, BS>> {
        use fil_actor_miner_state::v12::SECTORS_AMT_BITWIDTH;

        let mut out_array =
            ArrayNew::<fil_actor_miner_state::v12::SectorOnChainInfo, _>::new_with_bit_width(
                store,
                SECTORS_AMT_BITWIDTH,
            );

        in_array.for_each(|key, info_v11| {
            out_array.set(key, TypeMigrator::migrate_type(info_v11.clone(), store)?)?;
            Ok(())
        })?;

        Ok(out_array)
    }

    fn migrate_deadlines<BS: Blockstore>(
        &self,
        store: &BS,
        cache: &MigrationCache,
        address: &Address,
        deadlines: &Cid,
    ) -> anyhow::Result<Cid> {
        if deadlines == &self.empty_deadlines_v11 {
            return Ok(self.empty_deadlines_v12);
        }

        let in_deadlines = store
            .get_cbor::<DeadlinesOld>(deadlines)?
            .context("failed to get in_deadlines")?;
        let mut out_deadlines = DeadlinesNew::new(&self.policy_new, self.empty_deadline_v12);

        for (i, deadline) in in_deadlines.due.iter().enumerate() {
            if deadline == &self.empty_deadline_v11 {
                if i < out_deadlines.due.len() {
                    out_deadlines.due[i] = self.empty_deadline_v12;
                } else {
                    out_deadlines.due.push(self.empty_deadline_v12);
                }
            } else {
                let in_deadline = store
                    .get_cbor::<DeadlineOld>(deadline)?
                    .context("failed to get in_deadline")?;

                let out_sectors_snapshot_cid_cache_key =
                    sectors_amt_key(&in_deadline.sectors_snapshot)?;

                let out_sectors_snapshot_cid = cache.get_or_insert_with(
                    out_sectors_snapshot_cid_cache_key,
                    || -> anyhow::Result<Cid> {
                        let prev_in_root = cache.get(&miner_prev_sectors_in_key(address));
                        let prev_out_root = cache.get(&miner_prev_sectors_out_key(address));

                        if let (Some(prev_in_root), Some(prev_out_root)) =
                            (prev_in_root, prev_out_root)
                        {
                            self.migrate_sectors_with_diff(
                                store,
                                &in_deadline.sectors_snapshot,
                                &prev_in_root,
                                &prev_out_root,
                            )
                        } else {
                            let in_sector_snapshot =
                                ArrayOld::load(&in_deadline.sectors_snapshot, store)?;

                            let mut out_snapshot =
                                self.migrate_sectors_from_scratch(store, &in_sector_snapshot)?;

                            Ok(out_snapshot.flush()?)
                        }
                    },
                )?;

                let out_deadline = DeadlineNew {
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

                let out_deadline = store.put_cbor_default(&out_deadline)?;
                if i < out_deadlines.due.len() {
                    out_deadlines.due[i] = out_deadline;
                } else {
                    out_deadlines.due.push(out_deadline);
                }
            }
        }
        store.put_cbor_default(&out_deadlines)
    }
}

fn sectors_amt_key(sectors: &Cid) -> anyhow::Result<String> {
    let key = sectors.to_string_of_base(Base::Base32Lower)?;
    Ok(format!("sectorsAmt-{}", key))
}

fn miner_prev_sectors_in_key(address: &Address) -> String {
    format!("prevSectorsIn-{}", address)
}

fn miner_prev_sectors_out_key(address: &Address) -> String {
    format!("prevSectorsOut-{}", address)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::networks::{ChainConfig, Height};
    use crate::shim::{
        econ::TokenAmount,
        machine::{BuiltinActor, BuiltinActorManifest},
        state_tree::{ActorState, StateTree, StateTreeVersion},
    };
    use cid::multihash::MultihashDigest;
    use fvm_ipld_encoding::IPLD_RAW;
    use fvm_shared2::bigint::Zero;

    #[test]
    fn test_nv21_miner_migration() {
        let store = Arc::new(crate::db::MemoryDB::default());
        let (mut state_tree_old, manifest_old) = make_input_tree(&store);
        let system_actor_old = state_tree_old
            .get_actor(&fil_actor_interface::system::ADDRESS.into())
            .unwrap()
            .unwrap();
        let system_state_old: fil_actor_system_state::v11::State =
            store.get_cbor(&system_actor_old.state).unwrap().unwrap();
        let manifest_data_cid_old = system_state_old.builtin_actors;
        assert_eq!(manifest_data_cid_old, manifest_old.source_cid());

        let addr_id = 10000;
        let addr = Address::new_id(addr_id);
        let worker_id = addr_id + 100;

        // base stuff to create miners
        let miner_cid_old = manifest_old.get(BuiltinActor::Miner).unwrap();
        let mut miner_state1 = make_base_miner_state(&store, addr_id, worker_id);
        let mut deadline = DeadlineOld::new(&store).unwrap();
        let mut sectors_snapshot =
            ArrayOld::<fil_actor_miner_state::v11::SectorOnChainInfo, _>::new_with_bit_width(
                &store,
                fil_actor_miner_state::v11::SECTORS_AMT_BITWIDTH,
            );
        sectors_snapshot
            .set(
                0,
                fil_actor_miner_state::v11::SectorOnChainInfo {
                    simple_qa_power: true,
                    ..Default::default()
                },
            )
            .unwrap();
        sectors_snapshot
            .set(
                1,
                fil_actor_miner_state::v11::SectorOnChainInfo {
                    simple_qa_power: false,
                    ..Default::default()
                },
            )
            .unwrap();
        deadline.sectors_snapshot = sectors_snapshot.flush().unwrap();
        let deadline_cid = store.put_cbor_default(&deadline).unwrap();
        let deadlines = DeadlinesOld::new(
            &fil_actors_shared::v11::runtime::Policy::calibnet(),
            deadline_cid,
        );
        miner_state1.deadlines = store.put_cbor_default(&deadlines).unwrap();

        let miner1_state_cid = store.put_cbor_default(&miner_state1).unwrap();
        let miner1 = ActorState::new(miner_cid_old, miner1_state_cid, Zero::zero(), 0, None);
        state_tree_old.set_actor(&addr, miner1).unwrap();
        let tree_root = state_tree_old.flush().unwrap();

        let (new_manifest_cid, _new_manifest) = make_test_manifest(&store, "fil/12/");

        let mut chain_config = ChainConfig::devnet();
        if let Some(bundle) = &mut chain_config.height_infos[Height::Watermelon as usize].bundle {
            *bundle = new_manifest_cid;
        }
        let new_state_cid =
            super::super::run_migration(&chain_config, &store, &tree_root, 200).unwrap();

        let new_state_cid2 =
            super::super::run_migration(&chain_config, &store, &tree_root, 200).unwrap();

        assert_eq!(new_state_cid, new_state_cid2);

        let new_state_tree = StateTree::new_from_root(store.clone(), &new_state_cid).unwrap();
        let new_miner_state_cid = new_state_tree.get_actor(&addr).unwrap().unwrap().state;
        let new_miner_state: fil_actor_miner_state::v12::State =
            store.get_cbor(&new_miner_state_cid).unwrap().unwrap();
        let deadlines: fil_actor_miner_state::v12::Deadlines =
            store.get_cbor(&new_miner_state.deadlines).unwrap().unwrap();
        deadlines
            .for_each(&store, |_, deadline| {
                let sectors_snapshots =
                    ArrayNew::<fil_actor_miner_state::v12::SectorOnChainInfo, _>::load(
                        &deadline.sectors_snapshot,
                        &store,
                    )
                    .unwrap();
                assert_eq!(
                    sectors_snapshots.get(0).unwrap().unwrap().flags,
                    fil_actor_miner_state::v12::SectorOnChainInfoFlags::SIMPLE_QA_POWER
                );
                assert!(!sectors_snapshots
                    .get(1)
                    .unwrap()
                    .unwrap()
                    .flags
                    .contains(fil_actor_miner_state::v12::SectorOnChainInfoFlags::SIMPLE_QA_POWER));
                Ok(())
            })
            .unwrap();
    }

    fn make_input_tree<BS: Blockstore>(store: &Arc<BS>) -> (StateTree<BS>, BuiltinActorManifest) {
        let mut tree = StateTree::new(store.clone(), StateTreeVersion::V5).unwrap();

        let (_manifest_cid, manifest) = make_test_manifest(&store, "fil/11/");
        let system_cid = manifest.get_system();
        let system_state = fil_actor_system_state::v11::State {
            builtin_actors: manifest.source_cid(),
        };
        let system_state_cid = store.put_cbor_default(&system_state).unwrap();
        init_actor(
            &mut tree,
            system_state_cid,
            system_cid,
            &fil_actor_interface::system::ADDRESS.into(),
            Zero::zero(),
        );

        let init_cid = manifest.get_init();
        let init_state =
            fil_actor_init_state::v11::State::new(&store, "migrationtest".into()).unwrap();
        let init_state_cid = store.put_cbor_default(&init_state).unwrap();
        init_actor(
            &mut tree,
            init_state_cid,
            init_cid,
            &fil_actor_interface::init::ADDRESS.into(),
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
            let hash = cid::multihash::Code::Identity.digest(format!("{prefix}{name}").as_bytes());
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
        owner: fvm_shared3::ActorID,
        worker: fvm_shared3::ActorID,
    ) -> fil_actor_miner_state::v11::State {
        let control_addresses = vec![];
        let peer_id = vec![];
        let multi_address = vec![];
        let window_post_proof_type =
            fvm_shared3::sector::RegisteredPoStProof::StackedDRGWindow2KiBV1;
        let miner_info = fil_actor_miner_state::v11::MinerInfo::new(
            owner,
            worker,
            control_addresses,
            peer_id,
            multi_address,
            window_post_proof_type,
        )
        .unwrap();

        let miner_info_cid = store.put_cbor_default(&miner_info).unwrap();

        fil_actor_miner_state::v11::State::new(
            &fil_actors_shared::v11::runtime::Policy::calibnet(),
            store,
            miner_info_cid,
            0,
            0,
        )
        .unwrap()
    }
}

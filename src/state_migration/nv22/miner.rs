// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV22` upgrade for the
//! Miner actor.

use std::sync::Arc;

use crate::shim::econ::TokenAmount;
use crate::{
    shim::address::Address, state_migration::common::MigrationCache, utils::db::CborStoreExt,
};
use ahash::HashMap;
use anyhow::{bail, Context};
use cid::Cid;
use fil_actor_miner_state::{v12::State as MinerStateOld, v13::State as MinerStateNew};
use fil_actors_shared::fvm_ipld_amt;
use fil_actors_shared::v12::{runtime::Policy as PolicyOld, Array as ArrayOld};
use fil_actors_shared::v13::{runtime::Policy as PolicyNew, Array as ArrayNew};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use fvm_shared4::deal::DealID;
use fvm_shared4::sector::SectorID;
use parking_lot::RwLock;
use tracing::error;

use crate::state_migration::common::{
    ActorMigration, ActorMigrationInput, ActorMigrationOutput, TypeMigration, TypeMigrator,
};

#[derive(Default)]
pub struct ProviderSectors {
    pub deal_to_sector: RwLock<HashMap<DealID, SectorID>>,
}

pub struct MinerMigrator {
    provider_sectors: Arc<ProviderSectors>,
    policy_new: PolicyNew,
    out_cid: Cid,
}

pub(in crate::state_migration) fn miner_migrator<BS: Blockstore>(
    provider_sectors: Arc<ProviderSectors>,
    policy_old: &PolicyOld,
    policy_new: &PolicyNew,
    store: &Arc<BS>,
    out_cid: Cid,
) -> anyhow::Result<Arc<dyn ActorMigration<BS> + Send + Sync>> {
    Ok(Arc::new(MinerMigrator {
        provider_sectors,
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
        let miner_id = input.address.id()?;
        let in_state: MinerStateOld = store
            .get_cbor(&input.head)?
            .context("Miner actor: could not read v12 state")?;

        let in_sectors = ArrayOld::<fil_actor_miner_state::v12::SectorOnChainInfo, BS>::load(
            &in_state.sectors,
            store,
        )?;

        if let Some(prev_sectors_cid) = input.cache.get(&miner_prev_sectors_in_key(&input.address))
        {
            let prev_sectors = ArrayOld::<fil_actor_miner_state::v12::SectorOnChainInfo, BS>::load(
                &prev_sectors_cid,
                store,
            )?;
            let diffs = fvm_ipld_amt::diff(&prev_sectors, &in_sectors)?;

            for change in diffs.iter() {
                let sector_number = change.key;
                match change.change_type() {
                    fvm_ipld_amt::ChangeType::Add => {
                        let sector = in_sectors
                            .get(sector_number)?
                            .context("Failed to get sector")?;

                        if sector.deal_ids.is_empty() {
                            continue;
                        }

                        let mut sectors = self.provider_sectors.deal_to_sector.write();
                        for deal_id in &sector.deal_ids {
                            sectors.insert(
                                *deal_id,
                                SectorID {
                                    miner: miner_id,
                                    number: sector_number,
                                },
                            );
                        }
                    }
                    fvm_ipld_amt::ChangeType::Modify => {
                        // OhSnap deals
                        let sector_old = change.before.as_ref().context("Failed to get sector")?;
                        let sector_new = change.after.as_ref().context("Failed to get sector")?;

                        if sector_old.deal_ids.len() != sector_new.deal_ids.len() {
                            if !sector_old.deal_ids.is_empty() {
                                error!("old sector: {sector_old:?}, new_sector {sector_new:?}");
                                bail!("This is not supported, and should not happen");
                            }

                            let mut sectors = self.provider_sectors.deal_to_sector.write();
                            for deal_id in &sector_new.deal_ids {
                                sectors.insert(
                                    *deal_id,
                                    SectorID {
                                        miner: miner_id,
                                        number: sector_number,
                                    },
                                );
                            }
                        }
                    }
                    fvm_ipld_amt::ChangeType::Remove => {
                        // Comment from the Go implementation
                        // > nothing to do here, market removes deals based on activation/slash status, and can tell what
                        // > mappings to remove because non-slashed deals already had them
                    }
                };
            }
        } else {
            // there is no cached migration, so we iterate over all sectors and collect the deal
            // ids.
            in_sectors.for_each(|_, sector| {
                if sector.deal_ids.is_empty() {
                    return Ok(());
                }

                let mut sectors = self.provider_sectors.deal_to_sector.write();
                for (sector_number, deal_id) in sector.deal_ids.iter().enumerate() {
                    sectors.insert(
                        *deal_id,
                        SectorID {
                            miner: miner_id,
                            number: sector_number as u64,
                        },
                    );
                }

                Ok(())
            })?;
        }

        input
            .cache
            .insert(miner_prev_sectors_in_key(&input.address), in_state.sectors);

        Ok(Some(ActorMigrationOutput {
            new_code_cid: self.out_cid,
            new_head: input.head,
        }))
    }
}

fn miner_prev_sectors_in_key(address: &Address) -> String {
    format!("prevSectorsIn-{}", address)
}

//#[cfg(test)]
//mod tests {
//    use super::*;
//    use crate::networks::{ChainConfig, Height};
//    use crate::shim::{
//        econ::TokenAmount,
//        machine::{BuiltinActor, BuiltinActorManifest},
//        state_tree::{ActorState, StateTree, StateTreeVersion},
//    };
//    use cid::multihash::MultihashDigest;
//    use fvm_ipld_encoding::IPLD_RAW;
//    use fvm_shared2::bigint::Zero;
//
//    #[test]
//    fn test_nv21_miner_migration() {
//        let store = Arc::new(crate::db::MemoryDB::default());
//        let (mut state_tree_old, manifest_old) = make_input_tree(&store);
//        let system_actor_old = state_tree_old
//            .get_actor(&fil_actor_interface::system::ADDRESS.into())
//            .unwrap()
//            .unwrap();
//        let system_state_old: fil_actor_system_state::v11::State =
//            store.get_cbor(&system_actor_old.state).unwrap().unwrap();
//        let manifest_data_cid_old = system_state_old.builtin_actors;
//        assert_eq!(manifest_data_cid_old, manifest_old.source_cid());
//
//        let addr_id = 10000;
//        let addr = Address::new_id(addr_id);
//        let worker_id = addr_id + 100;
//
//        // base stuff to create miners
//        let miner_cid_old = manifest_old.get(BuiltinActor::Miner).unwrap();
//        let mut miner_state1 = make_base_miner_state(&store, addr_id, worker_id);
//        let mut deadline = DeadlineOld::new(&store).unwrap();
//        let mut sectors_snapshot =
//            ArrayOld::<fil_actor_miner_state::v11::SectorOnChainInfo, _>::new_with_bit_width(
//                &store,
//                fil_actor_miner_state::v11::SECTORS_AMT_BITWIDTH,
//            );
//        sectors_snapshot
//            .set(
//                0,
//                fil_actor_miner_state::v11::SectorOnChainInfo {
//                    simple_qa_power: true,
//                    ..Default::default()
//                },
//            )
//            .unwrap();
//        sectors_snapshot
//            .set(
//                1,
//                fil_actor_miner_state::v11::SectorOnChainInfo {
//                    simple_qa_power: false,
//                    ..Default::default()
//                },
//            )
//            .unwrap();
//        deadline.sectors_snapshot = sectors_snapshot.flush().unwrap();
//        let deadline_cid = store.put_cbor_default(&deadline).unwrap();
//        let deadlines = DeadlinesOld::new(
//            &fil_actors_shared::v11::runtime::Policy::calibnet(),
//            deadline_cid,
//        );
//        miner_state1.deadlines = store.put_cbor_default(&deadlines).unwrap();
//
//        let miner1_state_cid = store.put_cbor_default(&miner_state1).unwrap();
//        let miner1 = ActorState::new(miner_cid_old, miner1_state_cid, Zero::zero(), 0, None);
//        state_tree_old.set_actor(&addr, miner1).unwrap();
//        let tree_root = state_tree_old.flush().unwrap();
//
//        let (new_manifest_cid, _new_manifest) = make_test_manifest(&store, "fil/12/");
//
//        let mut chain_config = ChainConfig::devnet();
//        if let Some(bundle) = &mut chain_config.height_infos[Height::Watermelon as usize].bundle {
//            *bundle = new_manifest_cid;
//        }
//        let new_state_cid =
//            super::super::run_migration(&chain_config, &store, &tree_root, 200).unwrap();
//
//        let new_state_cid2 =
//            super::super::run_migration(&chain_config, &store, &tree_root, 200).unwrap();
//
//        assert_eq!(new_state_cid, new_state_cid2);
//
//        let new_state_tree = StateTree::new_from_root(store.clone(), &new_state_cid).unwrap();
//        let new_miner_state_cid = new_state_tree.get_actor(&addr).unwrap().unwrap().state;
//        let new_miner_state: fil_actor_miner_state::v12::State =
//            store.get_cbor(&new_miner_state_cid).unwrap().unwrap();
//        let deadlines: fil_actor_miner_state::v12::Deadlines =
//            store.get_cbor(&new_miner_state.deadlines).unwrap().unwrap();
//        deadlines
//            .for_each(&store, |_, deadline| {
//                let sectors_snapshots =
//                    ArrayNew::<fil_actor_miner_state::v12::SectorOnChainInfo, _>::load(
//                        &deadline.sectors_snapshot,
//                        &store,
//                    )
//                    .unwrap();
//                assert_eq!(
//                    sectors_snapshots.get(0).unwrap().unwrap().flags,
//                    fil_actor_miner_state::v12::SectorOnChainInfoFlags::SIMPLE_QA_POWER
//                );
//                assert!(!sectors_snapshots
//                    .get(1)
//                    .unwrap()
//                    .unwrap()
//                    .flags
//                    .contains(fil_actor_miner_state::v12::SectorOnChainInfoFlags::SIMPLE_QA_POWER));
//                Ok(())
//            })
//            .unwrap();
//    }
//
//    fn make_input_tree<BS: Blockstore>(store: &Arc<BS>) -> (StateTree<BS>, BuiltinActorManifest) {
//        let mut tree = StateTree::new(store.clone(), StateTreeVersion::V5).unwrap();
//
//        let (_manifest_cid, manifest) = make_test_manifest(&store, "fil/11/");
//        let system_cid = manifest.get_system();
//        let system_state = fil_actor_system_state::v11::State {
//            builtin_actors: manifest.source_cid(),
//        };
//        let system_state_cid = store.put_cbor_default(&system_state).unwrap();
//        init_actor(
//            &mut tree,
//            system_state_cid,
//            system_cid,
//            &fil_actor_interface::system::ADDRESS.into(),
//            Zero::zero(),
//        );
//
//        let init_cid = manifest.get_init();
//        let init_state =
//            fil_actor_init_state::v11::State::new(&store, "migrationtest".into()).unwrap();
//        let init_state_cid = store.put_cbor_default(&init_state).unwrap();
//        init_actor(
//            &mut tree,
//            init_state_cid,
//            init_cid,
//            &fil_actor_interface::init::ADDRESS.into(),
//            Zero::zero(),
//        );
//
//        tree.flush().unwrap();
//
//        (tree, manifest)
//    }
//
//    fn init_actor<BS: Blockstore>(
//        tree: &mut StateTree<BS>,
//        state: Cid,
//        code: Cid,
//        addr: &Address,
//        balance: TokenAmount,
//    ) {
//        let actor = ActorState::new(code, state, balance, 0, None);
//        tree.set_actor(addr, actor).unwrap();
//    }
//
//    fn make_test_manifest<BS: Blockstore>(store: &BS, prefix: &str) -> (Cid, BuiltinActorManifest) {
//        let mut manifest_data = vec![];
//        for name in [
//            "account",
//            "cron",
//            "init",
//            "storagemarket",
//            "storageminer",
//            "multisig",
//            "paymentchannel",
//            "storagepower",
//            "reward",
//            "system",
//            "verifiedregistry",
//            "datacap",
//        ] {
//            let hash = cid::multihash::Code::Identity.digest(format!("{prefix}{name}").as_bytes());
//            let code_cid = Cid::new_v1(IPLD_RAW, hash);
//            manifest_data.push((name, code_cid));
//        }
//
//        let manifest_cid = store
//            .put_cbor_default(&(1, store.put_cbor_default(&manifest_data).unwrap()))
//            .unwrap();
//        let manifest = BuiltinActorManifest::load_manifest(store, &manifest_cid).unwrap();
//
//        (manifest_cid, manifest)
//    }
//
//    fn make_base_miner_state<BS: Blockstore>(
//        store: &BS,
//        owner: fvm_shared3::ActorID,
//        worker: fvm_shared3::ActorID,
//    ) -> fil_actor_miner_state::v11::State {
//        let control_addresses = vec![];
//        let peer_id = vec![];
//        let multi_address = vec![];
//        let window_post_proof_type =
//            fvm_shared3::sector::RegisteredPoStProof::StackedDRGWindow2KiBV1;
//        let miner_info = fil_actor_miner_state::v11::MinerInfo::new(
//            owner,
//            worker,
//            control_addresses,
//            peer_id,
//            multi_address,
//            window_post_proof_type,
//        )
//        .unwrap();
//
//        let miner_info_cid = store.put_cbor_default(&miner_info).unwrap();
//
//        fil_actor_miner_state::v11::State::new(
//            &fil_actors_shared::v11::runtime::Policy::calibnet(),
//            store,
//            miner_info_cid,
//            0,
//            0,
//        )
//        .unwrap()
//    }
//}

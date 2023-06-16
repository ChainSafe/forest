// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV17` upgrade for the miner
//! actor.

use std::sync::Arc;

use ahash::HashMap;
use anyhow::Context;
use cid::{multibase::Base, Cid};
use fil_actor_miner_state::{
    v8::State as MinerStateOld,
    v9::{util::sector_key, State as MinerStateNew},
};
use fil_actors_shared::abi::commp::compute_unsealed_sector_cid_v2;
use forest_networks::{ChainConfig, NetworkChain};
use forest_shim::{address::Address, piece::piece_v2};
use forest_utils::db::CborStoreExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;

use crate::common::{
    ActorMigration, ActorMigrationInput, ActorMigrationOutput, TypeMigration, TypeMigrator,
};

pub struct MinerMigrator {
    network: NetworkChain,
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
    chain_config: &ChainConfig,
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

    let policy = match &chain_config.network {
        NetworkChain::Mainnet => fil_actors_shared::v8::runtime::Policy::mainnet(),
        NetworkChain::Calibnet => fil_actors_shared::v8::runtime::Policy::calibnet(),
        NetworkChain::Devnet(_) => unimplemented!("Policy::devnet"),
    };
    let empty_deadlines_v8 =
        fil_actor_miner_state::v8::Deadlines::new(&policy, empty_deadline_v8_cid);
    let empty_deadlines_v8_cid = store.put_cbor_default(&empty_deadlines_v8)?;

    let empty_deadline_v9 = fil_actor_miner_state::v9::Deadline::new(store)?;
    let empty_deadline_v9_cid = store.put_cbor_default(&empty_deadline_v9)?;

    let policy = match &chain_config.network {
        NetworkChain::Mainnet => fil_actors_shared::v9::runtime::Policy::mainnet(),
        NetworkChain::Calibnet => fil_actors_shared::v9::runtime::Policy::calibnet(),
        NetworkChain::Devnet(_) => unimplemented!("Policy::devnet"),
    };
    let empty_deadlines_v9 =
        fil_actor_miner_state::v9::Deadlines::new(&policy, empty_deadline_v9_cid);
    let empty_deadlines_v9_cid = store.put_cbor_default(&empty_deadlines_v9)?;

    Ok(Arc::new(MinerMigrator {
        network: chain_config.network.clone(),
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

            let mut out_array = if let Some(prev_in_root) = prev_in_root {
                if let Some(prev_out_root) = prev_out_root {
                    // we have previous work, but the AMT has changed -- diff them
                    let prev_in_sectors = fil_actors_shared::v8::Array::<
                        fil_actor_miner_state::v8::SectorOnChainInfo,
                        _,
                    >::load(prev_in_root, store)?;
                    let in_sectors = fil_actors_shared::v8::Array::<
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
                        match &change.change_type {
                            ChangeType::Remove => {
                                prev_out_sectors.delete(change.key)?;
                            }
                            // TODO: Double confirm `fallthrough` in `Go` is translated properly here
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

            cache.insert(key, out_root.clone());

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
            Ok(*&self.empty_deadline_v9_cid)
        } else {
            let in_deadlines: fil_actor_miner_state::v8::Deadlines = store
                .get_cbor(deadlines)?
                .context("Failed to get in_deadlines")?;

            let policy = match &self.network {
                NetworkChain::Mainnet => fil_actors_shared::v9::runtime::Policy::mainnet(),
                NetworkChain::Calibnet => fil_actors_shared::v9::runtime::Policy::calibnet(),
                NetworkChain::Devnet(_) => unimplemented!("Policy::devnet"),
            };
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
                                    out_sectors_snapshot_cid.clone(),
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::nv17::ManifestOld;

    use super::*;
    use anyhow::*;
    use cid::multihash::MultihashDigest;
    use fil_actor_interface::BURNT_FUNDS_ACTOR_ADDR;
    use forest_shim::{
        econ::TokenAmount,
        machine::ManifestV2,
        state_tree::{ActorState, StateTree, StateTreeVersion},
    };
    use fvm_ipld_blockstore::MemoryBlockstore;
    use fvm_ipld_encoding::IPLD_RAW;
    use fvm_shared::{bigint::Zero, state::StateRoot};

    #[test]
    fn test_nv17_miner_migration() -> Result<()> {
        let store = MemoryBlockstore::new();
        make_input_tree(&store)?;

        Ok(())
    }

    fn make_input_tree<BS: Blockstore + Clone>(store: BS) -> Result<Cid> {
        let mut tree = StateTree::new(store.clone(), StateTreeVersion::V4)?;

        let (manifest_cid, manifest_data_cid, manifest) = make_test_manifest(&store, "fil/8/")?;
        let account_cid = manifest.get_account_code();
        // fmt.Printf("accountCid: %s\n", accountCid)
        ensure!(account_cid.to_string() == "bafkqadlgnfwc6obpmfrwg33vnz2a");
        let system_cid = manifest.get_system_code();
        // fmt.Printf("systemCid: %s\n", systemCid)
        ensure!(system_cid.to_string() == "bafkqaddgnfwc6obpon4xg5dfnu");
        let system_state = fil_actor_system_state::v9::State {
            builtin_actors: manifest_data_cid,
        };
        let system_state_cid = store.put_cbor_default(&system_state)?;
        ensure!(
            system_state_cid.to_string()
                == "bafy2bzacebrujchvrqxwiml3aaud4ts7kgj74kkf7qewwmrsj5tvhneeamtlq"
        );
        init_actor(
            &mut tree,
            system_state_cid,
            *system_cid,
            &fil_actor_interface::system::ADDRESS.into(),
            Zero::zero(),
        )?;

        let init_cid = manifest.get_init_code();
        // fmt.Printf("initCid: %s\n", initCid)
        ensure!(init_cid.to_string() == "bafkqactgnfwc6obpnfxgs5a");
        let init_state = fil_actor_init_state::v8::State::new(&store, "migrationtest".into())?;
        let init_state_cid = store.put_cbor_default(&init_state)?;
        ensure!(
            init_state_cid.to_string()
                == "bafy2bzacednf3o3eyjwkm2isixe5lezt6klncgz5axriewegbkw34r6pqszbe"
        );
        init_actor(
            &mut tree,
            init_state_cid,
            *init_cid,
            &fil_actor_interface::init::ADDRESS.into(),
            Zero::zero(),
        )?;

        // Missing rust API, hard-coded here.
        // fmt.Printf("rewardCid: %s\n", rewardCid)
        let reward_cid = Cid::from_str("bafkqaddgnfwc6obpojsxoylsmq")?;
        let reward_state = fil_actor_reward_state::v8::State::new(Default::default());
        let reward_state_cid = store.put_cbor_default(&reward_state)?;
        ensure!(
            reward_state_cid.to_string()
                == "bafy2bzaceaslbmsgmgmfi6pn2osvqcfuqinauuyt67zifnefurhpk4zxd2fos"
        );
        init_actor(
            &mut tree,
            reward_state_cid,
            reward_cid,
            &fil_actor_interface::reward::ADDRESS.into(),
            TokenAmount::from_whole(1_100_000_000),
        )?;

        // Missing rust API, hard-coded here.
        // fmt.Printf("cronCid: %s\n", cronCid)
        let cron_cid = Cid::from_str("bafkqactgnfwc6obpmnzg63q")?;
        let cron_state = fil_actor_cron_state::v8::State {
            entries: vec![
                fil_actor_cron_state::v8::Entry {
                    receiver: fil_actor_interface::power::ADDRESS.into(),
                    method_num: fil_actor_interface::power::Method::OnEpochTickEnd as u64,
                },
                fil_actor_cron_state::v8::Entry {
                    receiver: fil_actor_interface::market::ADDRESS.into(),
                    method_num: fil_actor_interface::market::Method::CronTick as u64,
                },
            ],
        };
        let cron_state_cid = store.put_cbor_default(&cron_state)?;
        ensure!(
            cron_state_cid.to_string()
                == "bafy2bzacebs5dwwxmsjmzvoqcamx3qtl2x5qpqgpqxgnzl7scccmbvd37ulvs"
        );
        init_actor(
            &mut tree,
            cron_state_cid,
            cron_cid,
            &fil_actor_interface::cron::ADDRESS.into(),
            Zero::zero(),
        )?;
        // Missing rust API, hard-coded here.
        // fmt.Printf("powerCid: %s\n", powerCid)
        let power_cid = Cid::from_str("bafkqaetgnfwc6obpon2g64tbm5sxa33xmvza")?;
        let power_state = fil_actor_power_state::v8::State::new(&store)?;
        let power_state_cid = store.put_cbor_default(&power_state)?;
        ensure!(
            power_state_cid.to_string()
                == "bafy2bzacebx3h3ib435qrzwb7zj7enrgepqeiyyeqpq6zwygasoag4m3mhy3w"
        );
        init_actor(
            &mut tree,
            power_state_cid,
            power_cid,
            &fil_actor_interface::power::ADDRESS.into(),
            Zero::zero(),
        )?;

        // Missing rust API, hard-coded here.
        // fmt.Printf("marketCid: %s\n", marketCid)
        let market_cid = Cid::from_str("bafkqae3gnfwc6obpon2g64tbm5sw2ylsnnsxi")?;
        let market_state = fil_actor_market_state::v8::State::new(&store)?;
        let market_state_cid = store.put_cbor_default(&market_state)?;
        ensure!(
            market_state_cid.to_string()
                == "bafy2bzacea5udmevoj4io3yqy7ku7aitblugdvirbirg7wstzstb5xub5empc"
        );
        init_actor(
            &mut tree,
            market_state_cid,
            market_cid,
            &fil_actor_interface::market::ADDRESS.into(),
            Zero::zero(),
        )?;

        // this will need to be replaced with the address of a multisig actor for the verified registry to be tested accurately
        let verifreg_root = Address::new_id(80);
        let account_state = fil_actor_account_state::v8::State {
            address: verifreg_root.into(),
        };
        let account_state_cid = store.put_cbor_default(&account_state)?;
        ensure!(
            account_state_cid.to_string()
                == "bafy2bzaceajm42pledpxusdh4owdrdfvv463dthqg24npeeaz4jlbgzdcgkve"
        );
        init_actor(
            &mut tree,
            account_state_cid,
            *account_cid,
            &account_state.address.into(),
            Zero::zero(),
        )?;

        // Missing rust API, hard-coded here.
        // fmt.Printf("verifregCid: %s\n", verifregCid)
        let verifreg_cid = Cid::from_str("bafkqaftgnfwc6obpozsxe2lgnfswi4tfm5uxg5dspe")?;
        let verifreg_state =
            fil_actor_verifreg_state::v8::State::new(&store, verifreg_root.into())?;
        let verifreg_state_cid = store.put_cbor_default(&verifreg_state)?;
        ensure!(
            verifreg_state_cid.to_string()
                == "bafy2bzacea4jwfpd5vmqmq6y3qb5gnv4zv5nitpq5qkhvzzzqzd2hapcibwse"
        );
        init_actor(
            &mut tree,
            verifreg_state_cid,
            verifreg_cid,
            &fil_actors_shared::v8::builtin::VERIFIED_REGISTRY_ACTOR_ADDR.into(),
            Zero::zero(),
        )?;

        // burnt funds
        let account_state = fil_actor_account_state::v8::State {
            address: BURNT_FUNDS_ACTOR_ADDR,
        };
        let account_state_cid = store.put_cbor_default(&account_state)?;
        ensure!(
            account_state_cid.to_string()
                == "bafy2bzacedpuk5ggwoq3s2wixsyjjnexpsjstdlyntio76vs2lt2jvy3o6mau"
        );
        init_actor(
            &mut tree,
            account_state_cid,
            *account_cid,
            &account_state.address.into(),
            Zero::zero(),
        )?;

        let tree_root = tree.flush()?;
        println!("tree_root: {tree_root}");
        let state_root: StateRoot = store.get_cbor(&tree_root)?.unwrap();
        ensure!(
            state_root.actors.to_string()
                == "bafy2bzacecrfiicgwyogqfyovj5jld3oylod5ezp36tpyebwcuiz7wo3xxszy"
        );

        Ok(state_root.actors)
    }

    fn init_actor<BS: Blockstore + Clone>(
        tree: &mut StateTree<BS>,
        state: Cid,
        code: Cid,
        addr: &Address,
        balance: TokenAmount,
    ) -> Result<()> {
        let actor = ActorState::new(code, state, balance, 0, None);
        tree.set_actor(addr, actor)?;

        Ok(())
    }

    fn make_test_manifest<BS: Blockstore>(
        store: &BS,
        prefix: &str,
    ) -> Result<(Cid, Cid, ManifestV2)> {
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
        let manifest_data_cid = store.put_cbor_default(&manifest_data)?;
        // Output from Go: fmt.Printf("manifestDataCid:%s\n", manifestDataCid.String())
        ensure!(
            manifest_data_cid.to_string()
                == "bafy2bzaceb7wfqkjc5c3ccjyhaf7zuhkvbzpvhnb35feaettztoharc7zdndc"
        );

        let manifest = ManifestV2::new(manifest_data)?;
        let manifest_cid = store.put_cbor_default(&(1, manifest_data_cid))?;
        // Output from Go: fmt.Printf("manifestCid:%s\n", manifestCid.String())
        ensure!(
            manifest_cid.to_string()
                == "bafy2bzaceay4j73u6k2sqskjk6ru47v6l6uw2qyen77u47eduj4gbdmpqu65o"
        );

        Ok((manifest_cid, manifest_data_cid, manifest))
    }
}

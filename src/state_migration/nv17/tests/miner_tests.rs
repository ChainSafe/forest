// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
mod tests {
    use crate::networks::ChainConfig;
    use crate::networks::Height;
    use crate::shim::address::Address;
    use crate::shim::bigint::BigInt;
    use crate::shim::machine::{
        ACCOUNT_ACTOR_NAME, CRON_ACTOR_NAME, MARKET_ACTOR_NAME, MINER_ACTOR_NAME, POWER_ACTOR_NAME,
        REWARD_ACTOR_NAME, VERIFREG_ACTOR_NAME,
    };
    use crate::shim::{
        econ::TokenAmount,
        machine::Manifest,
        state_tree::{ActorState, StateTree, StateTreeVersion},
    };
    use crate::state_migration::nv17::migration::run_migration;
    use crate::utils::db::CborStoreExt;
    use anyhow::*;
    use cid::multihash::{Multihash, MultihashDigest};
    use cid::Cid;
    use fil_actor_interface::BURNT_FUNDS_ACTOR_ADDR;
    use fil_actor_miner_state::v9::util::sector_key;
    use fvm_ipld_blockstore::Blockstore;
    use fvm_ipld_encoding::CborStore;
    use fvm_ipld_encoding::IPLD_RAW;
    use fvm_ipld_hamt::BytesKey;
    use fvm_shared::{
        bigint::Zero,
        commcid::{
            FIL_COMMITMENT_SEALED, FIL_COMMITMENT_UNSEALED, POSEIDON_BLS12_381_A1_FC1,
            SHA2_256_TRUNC254_PADDED,
        },
        piece::PaddedPieceSize,
        state::StateRoot,
    };

    #[test]
    fn test_nv17_miner_migration() -> Result<()> {
        let store = crate::db::MemoryDB::default();
        let (mut state_tree_old, manifest_old) = make_input_tree(&store)?;
        let system_actor_old = state_tree_old
            .get_actor(&fil_actor_interface::system::ADDRESS.into())?
            .unwrap();
        let system_state_old: fil_actor_system_state::v9::State =
            store.get_cbor(&system_actor_old.state)?.unwrap();
        let manifest_data_cid_old = system_state_old.builtin_actors;
        ensure!(manifest_data_cid_old == manifest_old.actors_cid());
        ensure!(
            manifest_data_cid_old.to_string()
                == "bafy2bzaceb7wfqkjc5c3ccjyhaf7zuhkvbzpvhnb35feaettztoharc7zdndc"
        );

        let base_addr_id = 10000;
        let base_addr = Address::new_id(base_addr_id);
        let base_worker_addr = Address::new_id(base_addr_id + 100);

        // create 3 deal proposals
        let mut market_actor_old = state_tree_old
            .get_actor(&fil_actor_interface::market::ADDRESS.into())?
            .unwrap();
        let mut market_state_old: fil_actor_market_state::v8::State =
            store.get_cbor(&market_actor_old.state)?.unwrap();
        let mut proposals = fil_actors_shared::v8::Array::<
            fil_actor_market_state::v8::DealProposal,
            _,
        >::load(&market_state_old.proposals, &store)?;
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
            deal.piece_cid = make_piece_cid("0".as_bytes())?;
            ensure!(
                deal.piece_cid.to_string()
                    == "baga6ea4seaqf73hlm374q3zy3fjhq3dnnfwhtqw3yi452turwrtstvz2e75vp2i"
            );
            deal
        };
        let deal1 = {
            let mut deal = base_deal.clone();
            deal.piece_cid = make_piece_cid("1".as_bytes())?;
            ensure!(
                deal.piece_cid.to_string()
                    == "baga6ea4seaqgxbvsop7tj7hbtvvyatx7li7vor5nutvkely5jhab4uw5w6dvwsy"
            );
            deal
        };
        let deal2 = {
            let mut deal = base_deal;
            deal.piece_cid = make_piece_cid("2".as_bytes())?;
            ensure!(
                deal.piece_cid.to_string()
                    == "baga6ea4seaqni426hitf4fxo4a7vs4mltnoqgam4a7mlnri7sdnduzto5qj2wni"
            );
            deal
        };

        let mut pending_proposals =
            fil_actors_shared::v8::Set::from_root(&store, &market_state_old.pending_proposals)?;

        proposals.set(0, deal0)?;
        pending_proposals.put(BytesKey(deal1.cid()?.to_bytes()))?;
        proposals.set(1, deal1)?;
        pending_proposals.put(BytesKey(deal2.cid()?.to_bytes()))?;
        proposals.set(2, deal2)?;

        market_state_old.proposals = proposals.flush()?;
        ensure!(
            market_state_old.proposals.to_string()
                == "bafy2bzacecskt5brcfawiowjlv5lwnskkks2xzsmsnhkmjixndqlxuyb3abxs"
        );
        market_state_old.pending_proposals = pending_proposals.root()?;

        let market_state_cid_old = store.put_cbor_default(&market_state_old)?;
        market_actor_old.state = market_state_cid_old;
        state_tree_old.set_actor(
            &fil_actor_interface::market::ADDRESS.into(),
            market_actor_old,
        )?;

        // base stuff to create miners
        let miner_cid_old = manifest_old.code_by_name(MINER_ACTOR_NAME)?;
        ensure!(miner_cid_old.to_string() == "bafkqaetgnfwc6obpon2g64tbm5sw22lomvza");
        let base_miner_state = make_base_miner_state(&store, &base_addr, &base_worker_addr)?;

        let base_precommit = fil_actor_miner_state::v8::SectorPreCommitOnChainInfo {
            pre_commit_deposit: Zero::zero(),
            pre_commit_epoch: 0,
            deal_weight: Zero::zero(),
            verified_deal_weight: Zero::zero(),
            info: fil_actor_miner_state::v8::SectorPreCommitInfo {
                seal_proof: fvm_shared::sector::RegisteredSealProof::StackedDRG32GiBV1P1,
                sealed_cid: make_sealed_cid("100".as_bytes())?,
                ..Default::default()
            },
        };
        ensure!(
            base_precommit.info.sealed_cid.to_string()
                == "bagboea4b5abcblkxgzugketokvsj5szdvyourcdvislw57venjeowxmfu3xljuyg"
        );

        // make 3 miners
        // miner1 has no precommits at all
        // miner2 has 4 precommits, but with no deals
        // miner3 has 3 precommits, with deals [0], [1,2], and []

        // miner1 has no precommits at all
        let miner1_state_cid = store.put_cbor_default(&base_miner_state)?;
        ensure!(
            miner1_state_cid.to_string()
                == "bafy2bzaceaqtktd7f5b2gutreh3b2czp2mkqu4ilyuu7mjcpwrk75g5nl6w6k"
        );

        let miner1 = ActorState::new(*miner_cid_old, miner1_state_cid, Zero::zero(), 0, None);
        let addr1 = Address::new_id(base_addr_id + 1);
        state_tree_old.set_actor(&addr1, miner1)?;

        // miner2 has precommits, but they have no deals
        let mut precommits2 = fil_actors_shared::v8::make_map_with_root::<
            _,
            fil_actor_miner_state::v8::SectorPreCommitOnChainInfo,
        >(&base_miner_state.pre_committed_sectors, &store)?;
        precommits2.set(sector_key(0)?, base_precommit.clone())?;
        precommits2.set(sector_key(1)?, base_precommit.clone())?;
        precommits2.set(sector_key(2)?, base_precommit.clone())?;
        precommits2.set(sector_key(3)?, base_precommit.clone())?;

        let precommit2_cid = precommits2.flush()?;
        ensure!(
            precommit2_cid.to_string()
                == "bafy2bzacedogkdulyeaujgdsiqzo323s5dpz44efwihxsuekkkpo4znpl3g2s"
        );

        let mut miner2_state = base_miner_state.clone();
        miner2_state.pre_committed_sectors = precommit2_cid;
        let miner2_state_cid = store.put_cbor_default(&miner2_state)?;
        ensure!(
            miner2_state_cid.to_string()
                == "bafy2bzacedad6xkymehkuoij4rhg2inzqnfin3er52znw53lddn5364usp2bi"
        );

        let miner2 = ActorState::new(*miner_cid_old, miner2_state_cid, Zero::zero(), 0, None);
        let addr2 = Address::new_id(base_addr_id + 2);
        state_tree_old.set_actor(&addr2, miner2)?;

        // miner 3 has precommits, some of which have deals
        let mut precommits3 = fil_actors_shared::v8::make_map_with_root::<
            _,
            fil_actor_miner_state::v8::SectorPreCommitOnChainInfo,
        >(&base_miner_state.pre_committed_sectors, &store)?;
        let mut precommits3dot0 = base_precommit.clone();
        precommits3dot0.info.deal_ids = vec![0];
        precommits3dot0.info.sector_number = 0;

        let mut precommits3dot1 = base_precommit.clone();
        precommits3dot1.info.deal_ids = vec![1, 2];
        precommits3dot1.info.sector_number = 1;

        let mut precommits3dot2 = base_precommit;
        precommits3dot2.info.sector_number = 2;

        precommits3.set(sector_key(0)?, precommits3dot0)?;
        precommits3.set(sector_key(1)?, precommits3dot1)?;
        precommits3.set(sector_key(2)?, precommits3dot2)?;

        let precommits3_cid = precommits3.flush()?;
        ensure!(
            precommits3_cid.to_string()
                == "bafy2bzacecdpddgu5sxniq5iez3xapyxvi3dg7pc5oxthywuclvxyj4h2vweo"
        );

        let mut miner3_state = base_miner_state.clone();
        miner3_state.pre_committed_sectors = precommits3_cid;
        let miner3_state_cid = store.put_cbor_default(&miner3_state)?;
        ensure!(
            miner3_state_cid.to_string()
                == "bafy2bzaceb7ojujla7jb6iaxeuk4etg2kui4gtjujwqadqkc7lkp4ugoqrh2m"
        );

        let miner3 = ActorState::new(*miner_cid_old, miner3_state_cid, Zero::zero(), 0, None);
        let addr3 = Address::new_id(base_addr_id + 3);
        state_tree_old.set_actor(&addr3, miner3)?;

        let tree_root = state_tree_old.flush()?;

        let (new_manifest_cid, _new_manifest) = make_test_manifest(&store, "fil/9/")?;

        let mut chain_config = ChainConfig::calibnet();
        if let Some(bundle) = &mut chain_config.height_infos[Height::Shark as usize].bundle {
            bundle.manifest = new_manifest_cid;
        }
        let new_state_cid = run_migration(&chain_config, &store, &tree_root, 200)?;
        let actors_out_state_root: StateRoot = store.get_cbor(&new_state_cid)?.unwrap();
        ensure!(
            actors_out_state_root.actors.to_string()
                == "bafy2bzacedgtk3lnnyfxnzc32etqaj3zvi7ar7nxq2jtxd2qr36ftbsjoycqu"
        );
        let new_state_cid2 = run_migration(&chain_config, &store, &tree_root, 200)?;
        ensure!(new_state_cid == new_state_cid2);

        Ok(())
    }

    #[test]
    fn test_fip0029_miner_migration() -> Result<()> {
        let store = crate::db::MemoryDB::default();
        let (mut state_tree_old, manifest_old) = make_input_tree(&store)?;
        let addr = Address::new_id(10000);
        let worker_addr = Address::new_id(20000);
        let miner_cid_old = manifest_old.code_by_name(MINER_ACTOR_NAME)?;
        let miner_state = make_base_miner_state(&store, &addr, &worker_addr)?;
        let miner_state_cid = store.put_cbor_default(&miner_state)?;
        ensure!(
            miner_state_cid.to_string()
                == "bafy2bzaceacitm72b4zks7ivplygpmyqa6aas2pxkv4rkiknluxiko5mn4ng6"
        );
        let miner_actor = ActorState::new(*miner_cid_old, miner_state_cid, Zero::zero(), 0, None);
        state_tree_old.set_actor(&addr, miner_actor)?;
        let state_tree_old_root = state_tree_old.flush()?;
        let (new_manifest_cid, _new_manifest) = make_test_manifest(&store, "fil/9/")?;
        let mut chain_config = ChainConfig::calibnet();
        if let Some(bundle) = &mut chain_config.height_infos[Height::Shark as usize].bundle {
            bundle.manifest = new_manifest_cid;
        }
        let new_state_cid = run_migration(&chain_config, &store, &state_tree_old_root, 200)?;
        let actors_out_state_root: StateRoot = store.get_cbor(&new_state_cid)?.unwrap();
        ensure!(
            actors_out_state_root.actors.to_string()
                == "bafy2bzacebdpnjjyspbyj7al7d6234kdhkmdygkfdkp6zyao5o3egsfmribty"
        );

        Ok(())
    }

    fn make_input_tree<BS: Blockstore + Clone>(store: BS) -> Result<(StateTree<BS>, Manifest)> {
        let mut tree = StateTree::new(store.clone(), StateTreeVersion::V4)?;

        let (_manifest_cid, manifest) = make_test_manifest(&store, "fil/8/")?;
        let account_cid = manifest.code_by_name(ACCOUNT_ACTOR_NAME)?;
        // fmt.Printf("accountCid: %s\n", accountCid)
        ensure!(account_cid.to_string() == "bafkqadlgnfwc6obpmfrwg33vnz2a");
        let system_cid = manifest.system_code();
        // fmt.Printf("systemCid: %s\n", systemCid)
        ensure!(system_cid.to_string() == "bafkqaddgnfwc6obpon4xg5dfnu");
        let system_state = fil_actor_system_state::v9::State {
            builtin_actors: manifest.actors_cid(),
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

        let init_cid = manifest.init_code();
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

        let reward_cid = manifest.code_by_name(REWARD_ACTOR_NAME)?;
        ensure!(reward_cid.to_string() == "bafkqaddgnfwc6obpojsxoylsmq");
        let reward_state = fil_actor_reward_state::v8::State::new(Default::default());
        let reward_state_cid = store.put_cbor_default(&reward_state)?;
        ensure!(
            reward_state_cid.to_string()
                == "bafy2bzaceaslbmsgmgmfi6pn2osvqcfuqinauuyt67zifnefurhpk4zxd2fos"
        );
        init_actor(
            &mut tree,
            reward_state_cid,
            *reward_cid,
            &fil_actor_interface::reward::ADDRESS.into(),
            TokenAmount::from_whole(1_100_000_000),
        )?;

        let cron_cid = manifest.code_by_name(CRON_ACTOR_NAME)?;
        ensure!(cron_cid.to_string() == "bafkqactgnfwc6obpmnzg63q");
        let cron_state = fil_actor_cron_state::v8::State {
            entries: vec![
                fil_actor_cron_state::v8::Entry {
                    receiver: fil_actor_interface::power::ADDRESS,
                    method_num: fil_actor_interface::power::Method::OnEpochTickEnd as u64,
                },
                fil_actor_cron_state::v8::Entry {
                    receiver: fil_actor_interface::market::ADDRESS,
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
            *cron_cid,
            &fil_actor_interface::cron::ADDRESS.into(),
            Zero::zero(),
        )?;

        let power_cid = manifest.code_by_name(POWER_ACTOR_NAME)?;
        ensure!(power_cid.to_string() == "bafkqaetgnfwc6obpon2g64tbm5sxa33xmvza");
        let power_state = fil_actor_power_state::v8::State::new(&store)?;
        let power_state_cid = store.put_cbor_default(&power_state)?;
        ensure!(
            power_state_cid.to_string()
                == "bafy2bzacebx3h3ib435qrzwb7zj7enrgepqeiyyeqpq6zwygasoag4m3mhy3w"
        );
        init_actor(
            &mut tree,
            power_state_cid,
            *power_cid,
            &fil_actor_interface::power::ADDRESS.into(),
            Zero::zero(),
        )?;

        let market_cid = manifest.code_by_name(MARKET_ACTOR_NAME)?;
        ensure!(market_cid.to_string() == "bafkqae3gnfwc6obpon2g64tbm5sw2ylsnnsxi");
        let market_state = fil_actor_market_state::v8::State::new(&store)?;
        let market_state_cid = store.put_cbor_default(&market_state)?;
        ensure!(
            market_state_cid.to_string()
                == "bafy2bzacea5udmevoj4io3yqy7ku7aitblugdvirbirg7wstzstb5xub5empc"
        );
        init_actor(
            &mut tree,
            market_state_cid,
            *market_cid,
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

        let verifreg_cid = manifest.code_by_name(VERIFREG_ACTOR_NAME)?;
        ensure!(verifreg_cid.to_string() == "bafkqaftgnfwc6obpozsxe2lgnfswi4tfm5uxg5dspe");
        let mut verifreg_state =
            fil_actor_verifreg_state::v8::State::new(&store, verifreg_root.into())?;
        let mut verified_clients = fil_actors_shared::v8::make_empty_map::<BS, BigInt>(
            &store,
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
            verified_clients.set(
                BytesKey(Address::new_id(1001).to_bytes()),
                num_bigint::BigInt::from(1).into(),
            )?;
            verified_clients.set(
                BytesKey(Address::new_id(1002).to_bytes()),
                num_bigint::BigInt::from(2).into(),
            )?;
            verifreg_state.verified_clients = verified_clients.flush()?;
        }
        let verifreg_state_cid = store.put_cbor_default(&verifreg_state)?;
        init_actor(
            &mut tree,
            verifreg_state_cid,
            *verifreg_cid,
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

        tree.flush()?;

        Ok((tree, manifest))
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

    fn make_test_manifest<BS: Blockstore>(store: &BS, prefix: &str) -> Result<(Cid, Manifest)> {
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

        let manifest_cid = store.put_cbor_default(&(1, store.put_cbor_default(&manifest_data)?))?;
        let manifest = Manifest::load(store, &manifest_cid)?;

        Ok((manifest_cid, manifest))
    }

    fn make_base_miner_state<BS: Blockstore>(
        store: &BS,
        base_addr: &Address,
        base_worker_addr: &Address,
    ) -> Result<fil_actor_miner_state::v8::State> {
        let empty_miner_info = fil_actor_miner_state::v8::MinerInfo {
            owner: base_addr.into(),
            worker: base_worker_addr.into(),
            control_addresses: vec![],
            pending_worker_key: None,
            peer_id: vec![],
            multi_address: vec![],
            window_post_proof_type: fvm_shared::sector::RegisteredPoStProof::Invalid(0),
            sector_size: fvm_shared::sector::SectorSize::_2KiB, // 0 not available in rust API, change Go code to 2 << 10 and all tests still pass
            window_post_partition_sectors: 0,
            consensus_fault_elapsed: 0,
            pending_owner_address: None,
        };

        let empty_miner_info_cid = store.put_cbor_default(&empty_miner_info)?;

        let empty_miner_state = fil_actor_miner_state::v8::State::new(
            &fil_actors_shared::v8::runtime::Policy::calibnet(),
            store,
            empty_miner_info_cid,
            0,
            0,
        )?;

        Ok(empty_miner_state)
    }

    fn make_piece_cid(data: &[u8]) -> Result<Cid> {
        let hash = cid::multihash::Code::Sha2_256.digest(data);
        let hash = Multihash::wrap(SHA2_256_TRUNC254_PADDED, hash.digest())?;
        Ok(Cid::new_v1(FIL_COMMITMENT_UNSEALED, hash))
    }

    fn make_sealed_cid(data: &[u8]) -> Result<Cid> {
        let hash = cid::multihash::Code::Sha2_256.digest(data);
        let hash = Multihash::wrap(POSEIDON_BLS12_381_A1_FC1, hash.digest())?;
        Ok(Cid::new_v1(FIL_COMMITMENT_SEALED, hash))
    }
}

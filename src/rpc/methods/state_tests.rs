// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Synthetic (snapshot-free) coverage for `Filecoin.StateMinerCreationDeposit`
//! and the extracted `compute_initial_pledge_for_power`. The tests build a
//! state tree with the handful of actors the calculation reads and exercise it
//! at the real network-version-27 activation epoch.

use super::{StateMinerCreationDeposit, StoragePower, compute_initial_pledge_for_power};
use crate::blocks::{CachingBlockHeader, RawBlockHeader, Tipset};
use crate::chain::ChainStore;
use crate::db::{DbImpl, MemoryDB};
use crate::message_pool::{MessagePool, MpoolLocker, NonceTracker};
use crate::networks::{ACTOR_BUNDLES_METADATA, ChainConfig};
use crate::rpc::RPCState;
use crate::rpc::eth::filter::EthEventHandler;
use crate::rpc::reflect::RpcMethod as _;
use crate::rpc::types::ApiTipsetKey;
use crate::shim::actors::{power, reward};
use crate::shim::address::Address;
use crate::shim::econ::TokenAmount;
use crate::shim::machine::BuiltinActor;
use crate::shim::state_tree::{ActorState, StateTree, StateTreeVersion};
use crate::state_manager::StateManager;
use crate::utils::ShallowClone;
use crate::utils::db::CborStoreExt as _;
use cid::Cid;
use fil_actors_shared::v18::builtin::reward::smooth::FilterEstimate;
use fvm_ipld_blockstore::Blockstore;
use num::BigInt;
use std::str::FromStr as _;
use std::sync::Arc;

// First calibnet epoch on network version 27 (the deposit calculation): one
// past the GoldenWeek upgrade height. Testing here keeps the real upgrade
// schedule rather than overriding the genesis network version.
const CALIBNET_V27_EPOCH: i64 = 3_007_295;

// `power::State`/`reward::State::default_latest_version` are the shim
// constructors for the latest actor version; they take the actor-native
// `FilterEstimate` in their signature, so the test builds one here.
fn filter_estimate(position: i128) -> FilterEstimate {
    FilterEstimate {
        position: BigInt::from(position),
        velocity: BigInt::from(0),
    }
}

// Builds a state tree with the power, reward, burnt-funds and reserve actors
// that `compute_initial_pledge_for_power` and the circulating-supply
// calculation read, and returns its root. `ramp_start_epoch` selects the
// pledge-ramp branch under test.
fn build_state_root<S: Blockstore + ShallowClone>(
    store: &S,
    ramp_start_epoch: i64,
    ramp_duration_epochs: u64,
) -> Cid {
    let smoothed = 1i128 << 50;
    let power_state = power::State::default_latest_version(
        StoragePower::from(1u64 << 50),
        StoragePower::from(0),
        StoragePower::from(1u64 << 50),
        StoragePower::from(0),
        (&TokenAmount::from_whole(0)).into(), // total_pledge_collateral -> total_locked
        StoragePower::from(0),
        StoragePower::from(0),
        (&TokenAmount::from_whole(0)).into(),
        filter_estimate(smoothed), // this_epoch_qa_power_smoothed -> total_power_smoothed
        1,
        1,
        Cid::default(),
        0,
        Cid::default(),
        None,
        ramp_start_epoch,
        ramp_duration_epochs,
    );
    let reward_state = reward::State::default_latest_version(
        StoragePower::from(0),
        StoragePower::from(0),
        0,
        StoragePower::from(1u64 << 50),
        (&TokenAmount::from_whole(0)).into(),
        filter_estimate(smoothed), // this_epoch_reward_smoothed (estimate rounds to 0)
        // Baseline power well above `qa_power` so the ramp's baseline and simple
        // denominators differ and the ramp fraction actually affects the pledge.
        StoragePower::from(1u128 << 80), // this_epoch_baseline_power
        0,
        (&TokenAmount::from_whole(1_000_000)).into(), // total_storage_power_reward -> fil_mined
        (&TokenAmount::from_whole(0)).into(),
        (&TokenAmount::from_whole(0)).into(),
    );

    let meta = ACTOR_BUNDLES_METADATA
        .values()
        .find(|m| m.actor_major_version().ok() == Some(18))
        .expect("v18 actor bundle metadata is embedded");
    let power_code = meta.manifest.get(BuiltinActor::Power).unwrap();
    let reward_code = meta.manifest.get(BuiltinActor::Reward).unwrap();

    let power_cid = store.put_cbor_default(&power_state).unwrap();
    let reward_cid = store.put_cbor_default(&reward_state).unwrap();

    let zero = TokenAmount::from_whole(0);
    let mut tree = StateTree::new(store, StateTreeVersion::V5).unwrap();
    tree.set_actor(
        &Address::POWER_ACTOR,
        ActorState::new(power_code, power_cid, zero.clone(), 0, None),
    )
    .unwrap();
    tree.set_actor(
        &Address::REWARD_ACTOR,
        ActorState::new(reward_code, reward_cid, zero.clone(), 0, None),
    )
    .unwrap();
    // Only the balance of these two is read by the circulating-supply calc, so
    // dummy code/state CIDs are fine.
    tree.set_actor(
        &Address::BURNT_FUNDS_ACTOR,
        ActorState::new(Cid::default(), Cid::default(), zero.clone(), 0, None),
    )
    .unwrap();
    tree.set_actor(
        &Address::RESERVE_ACTOR,
        ActorState::new(Cid::default(), Cid::default(), zero, 0, None),
    )
    .unwrap();
    tree.flush().unwrap()
}

fn calibnet_state_manager() -> StateManager {
    let genesis = CachingBlockHeader::new(RawBlockHeader {
        miner_address: Address::new_id(0),
        timestamp: 7777,
        ..Default::default()
    });
    let db = Arc::new(MemoryDB::default());
    let cs = ChainStore::new(db, Arc::new(ChainConfig::calibnet()), genesis).unwrap();
    StateManager::new(cs).unwrap()
}

// A standalone tipset at an arbitrary epoch whose parent-state points at
// `state_root`; lets the compute path run at a post-genesis epoch without
// building a full chain.
fn tipset_at(epoch: i64, state_root: Cid) -> Tipset {
    Tipset::from(CachingBlockHeader::new(RawBlockHeader {
        miner_address: Address::new_id(0),
        state_root,
        epoch,
        ..Default::default()
    }))
}

fn atto(decimal: &str) -> TokenAmount {
    TokenAmount::from_atto(BigInt::from_str(decimal).unwrap())
}

#[test]
fn compute_initial_pledge_with_active_ramp() {
    let sm = calibnet_state_manager();
    let root = build_state_root(&sm.db_owned(), CALIBNET_V27_EPOCH - 100, 200);
    let ts = tipset_at(CALIBNET_V27_EPOCH, root);
    let qa_power = StoragePower::from(1u128 << 70);
    let pledge = compute_initial_pledge_for_power(&sm, &ts, &qa_power).unwrap();
    // Half way through the ramp; the pledge is about half the fully-ramped value
    // asserted in `compute_initial_pledge_without_ramp`, so the ramp parameters
    // demonstrably affect the result.
    assert_eq!(pledge, atto("71597844713531865507596274"));
}

#[test]
fn compute_initial_pledge_without_ramp() {
    let sm = calibnet_state_manager();
    let root = build_state_root(&sm.db_owned(), 0, 0);
    let ts = tipset_at(CALIBNET_V27_EPOCH, root);
    let qa_power = StoragePower::from(1u128 << 70);
    let pledge = compute_initial_pledge_for_power(&sm, &ts, &qa_power).unwrap();
    // Ramp start at 0 means no ramp: the fully-activated pledge.
    assert_eq!(pledge, atto("142732122934907487146577486"));
}

#[tokio::test]
async fn creation_deposit_is_zero_before_v27() {
    // Mainnet before the GoldenWeek (nv27) upgrade: the handler returns a zero
    // deposit without reading the state tree.
    let ctx = build_ctx(ChainConfig::mainnet(), Cid::default(), empty_db(), None);
    let deposit =
        StateMinerCreationDeposit::handle(ctx, (ApiTipsetKey(None),), &Default::default())
            .await
            .unwrap();
    assert_eq!(deposit, TokenAmount::from_whole(0));
}

#[tokio::test]
async fn creation_deposit_matches_pledge_at_v27() {
    // Populate a store and set a calibnet head at the nv27 activation epoch, so
    // the handler runs the real upgrade path rather than an overridden version.
    let db: DbImpl = Arc::new(MemoryDB::default()).into();
    let root = build_state_root(&db, 0, 0);
    let head = tipset_at(CALIBNET_V27_EPOCH, root);
    let ctx = build_ctx(ChainConfig::calibnet(), root, db, Some(head.clone()));

    let deposit =
        StateMinerCreationDeposit::handle(ctx.clone(), (ApiTipsetKey(None),), &Default::default())
            .await
            .unwrap();

    // The deposit is the initial pledge for one tenth of the minimum consensus
    // power, so it must match the direct calculation for that power.
    let deposit_power = &ctx.chain_config().policy.minimum_consensus_power / 10;
    let expected =
        compute_initial_pledge_for_power(&ctx.state_manager, &head, &deposit_power).unwrap();
    assert!(expected > TokenAmount::from_whole(0));
    assert_eq!(deposit, expected);
}

fn empty_db() -> DbImpl {
    Arc::new(MemoryDB::default()).into()
}

// Minimal RPCState over an in-memory chain whose genesis tipset uses
// `genesis_state_root`, optionally with `head` promoted to the heaviest tipset;
// mirrors the test context in `sync.rs`.
fn build_ctx(
    chain_config: ChainConfig,
    genesis_state_root: Cid,
    db: DbImpl,
    head: Option<Tipset>,
) -> Arc<RPCState> {
    use crate::chain_sync::network_context::SyncNetworkContext;
    use crate::key_management::{KeyStore, KeyStoreConfig};
    use crate::libp2p::{NetworkMessage, PeerManager};
    use parking_lot::RwLock;
    use tokio::sync::mpsc;
    use tokio::task::JoinSet;

    let (network_send, _network_rx) = flume::bounded::<NetworkMessage>(5);
    let (tipset_send, _tipset_rx) = flume::bounded(5);
    let mut services = JoinSet::new();
    let genesis = CachingBlockHeader::new(RawBlockHeader {
        miner_address: Address::new_id(0),
        state_root: genesis_state_root,
        timestamp: 7777,
        ..Default::default()
    });
    let cs = ChainStore::new(db, Arc::new(chain_config), genesis).unwrap();
    let state_manager = StateManager::new(cs.shallow_clone()).unwrap();
    let mpool = MessagePool::new(
        cs,
        network_send.clone(),
        Default::default(),
        state_manager.chain_config().clone(),
        &mut services,
    )
    .unwrap();
    // Promote the head on the chain store the handler reads from.
    if let Some(head) = head {
        state_manager
            .chain_store()
            .set_heaviest_tipset(head)
            .unwrap();
    }
    let peer_manager = Arc::new(PeerManager::default());
    let sync_network_context =
        SyncNetworkContext::new(network_send, peer_manager, state_manager.db_owned());
    Arc::new(RPCState {
        state_manager,
        keystore: Arc::new(RwLock::new(KeyStore::new(KeyStoreConfig::Memory).unwrap())),
        mpool,
        bad_blocks: Some(Default::default()),
        sync_status: Default::default(),
        eth_event_handler: Arc::new(EthEventHandler::new()),
        eth_logs_feed: Default::default(),
        sync_network_context,
        start_time: chrono::Utc::now(),
        shutdown: mpsc::channel(1).0,
        tipset_send,
        block_validation_subscriber: Default::default(),
        snapshot_progress_tracker: Default::default(),
        mpool_locker: MpoolLocker::new(),
        nonce_tracker: NonceTracker::new(),
        temp_dir: Arc::new(std::env::temp_dir()),
    })
}

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::peer_manager::PeerManager;
use actor::EPOCH_DURATION_SECONDS;
use async_std::sync::channel;
use async_std::task;
use beacon::{DrandBeacon, DrandPublic};
use db::MemoryDB;
use fil_types::verifier::FullVerifier;
use forest_car::load_car;
use forest_libp2p::{chain_exchange::make_chain_exchange_response, NetworkMessage};
use genesis::{initialize_genesis, EXPORT_SR_40};
use libp2p::core::PeerId;
use state_manager::StateManager;

async fn handle_requests<DB>(mut chan: Receiver<NetworkMessage>, db: ChainStore<DB>)
where
    DB: BlockStore + Send + Sync + 'static,
{
    loop {
        match chan.next().await {
            Some(NetworkMessage::ChainExchangeRequest {
                request,
                response_channel,
                ..
            }) => response_channel
                .send(make_chain_exchange_response(&db, &request).await)
                .unwrap(),
            Some(event) => log::warn!("Other request sent to network: {:?}", event),
            None => break,
        }
    }
}

#[async_std::test]
// Test is ignored because it relies on network requests for beacon access
#[ignore]
async fn space_race_full_sync() {
    pretty_env_logger::init();

    let db = Arc::new(MemoryDB::default());

    let chain_store = Arc::new(ChainStore::new(db.clone()));
    let state_manager = Arc::new(StateManager::new(chain_store));

    let (network_send, network_recv) = channel(20);

    // Initialize genesis using default (currently space-race) genesis
    let (genesis, _) = initialize_genesis(None, &state_manager).await.unwrap();
    let genesis = Arc::new(genesis);

    let beacon = Arc::new(DrandBeacon::new(
        "https://pl-us.incentinet.drand.sh",
        DrandPublic{coefficient: hex::decode("8cad0c72c606ab27d36ee06de1d5b2db1faf92e447025ca37575ab3a8aac2eaae83192f846fc9e158bc738423753d000").unwrap()},
        genesis.blocks()[0].timestamp(),
        EPOCH_DURATION_SECONDS as u64,
    )
    .await
    .unwrap());

    let peer = PeerId::random();
    let peer_manager = PeerManager::default();
    peer_manager.update_peer_head(peer, None).await;
    let network = SyncNetworkContext::new(network_send, Arc::new(peer_manager), db);

    let provider_db = Arc::new(MemoryDB::default());
    let cids: Vec<Cid> = load_car(provider_db.as_ref(), EXPORT_SR_40.as_ref())
        .await
        .unwrap();
    let prov_cs = ChainStore::new(provider_db);
    let target = prov_cs
        .tipset_from_keys(&TipsetKeys::new(cids))
        .await
        .unwrap();

    let worker = SyncWorker {
        state: Default::default(),
        beacon,
        state_manager,
        network,
        genesis,
        bad_blocks: Default::default(),
        verifier: PhantomData::<FullVerifier>::default(),
        req_window: 200,
    };

    // Setup process to handle requests from syncer
    task::spawn(async { handle_requests(network_recv, prov_cs).await });

    worker.sync(target).await.unwrap();
}

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::cli::{block_until_sigint, Config};
use actor::EPOCH_DURATION_SECONDS;
use async_std::sync::RwLock;
use async_std::task;
use auth::{generate_priv_key, JWT_IDENTIFIER};
use beacon::{DrandBeacon, DEFAULT_DRAND_URL};
use blocks::TipsetKeys;
use chain::ChainStore;
use chain_sync::ChainSyncer;
use db::RocksDb;
use encoding::Cbor;
use fil_types::verifier::{FullVerifier, ProofVerifier};
use flo_stream::{MessagePublisher, Publisher};
use forest_car::load_car;
use forest_libp2p::{get_keypair, Libp2pService};
use genesis::initialize_genesis;
use ipld_blockstore::BlockStore;
use libp2p::identity::{ed25519, Keypair};
use log::{debug, info, trace};
use message_pool::{MessagePool, MpoolConfig, MpoolRpcProvider};
use paramfetch::{get_params_default, SectorSizeOpt};
use rpc::{start_rpc, RpcState};
use state_manager::StateManager;
use std::fs::File;
use std::io::BufReader;
use std::io::Read;
use std::sync::Arc;
use utils::write_to_file;
use wallet::{KeyStore, PersistentKeyStore};

/// Number of tasks spawned for sync workers.
// TODO benchmark and/or add this as a config option. (1 is temporary value to avoid overlap)
const WORKER_TASKS: usize = 1;

/// Import a chain from a CAR file
async fn import_chain<V: ProofVerifier, R: Read, DB>(
    sm: &Arc<StateManager<DB>>,
    reader: R,
    snapshot: bool,
) -> Result<(), Box<dyn std::error::Error>>
where
    DB: BlockStore + Send + Sync + 'static,
{
    info!("Importing chain from snapshot");
    // start import
    let cids = load_car(sm.blockstore(), reader)?;
    let ts = sm.chain_store().tipset_from_keys(&TipsetKeys::new(cids))?;
    let gb = sm.chain_store().tipset_by_height(0, &ts, true)?.unwrap();
    if !snapshot {
        info!("Validating imported chain");
        sm.validate_chain::<V>(ts.clone()).await?;
    }
    let gen_cid = sm.chain_store().set_genesis(&gb.blocks()[0])?;
    sm.blockstore()
        .write(chain::HEAD_KEY, ts.key().marshal_cbor()?)?;
    info!(
        "Accepting {:?} as new head with genesis {:?}",
        ts.cids(),
        gen_cid
    );
    Ok(())
}

/// Starts daemon process
pub(super) async fn start(config: Config) {
    info!("Starting Forest daemon");
    let net_keypair = get_keypair(&format!("{}{}", &config.data_dir, "/libp2p/keypair"))
        .unwrap_or_else(|| {
            // Keypair not found, generate and save generated keypair
            let gen_keypair = ed25519::Keypair::generate();
            // Save Ed25519 keypair to file
            // TODO rename old file to keypair.old(?)
            if let Err(e) = write_to_file(
                &gen_keypair.encode(),
                &format!("{}{}", &config.data_dir, "/libp2p/"),
                "keypair",
            ) {
                info!("Could not write keystore to disk!");
                trace!("Error {:?}", e);
            };
            Keypair::Ed25519(gen_keypair)
        });

    // Initialize keystore
    let mut ks = PersistentKeyStore::new(config.data_dir.to_string()).unwrap();
    if ks.get(JWT_IDENTIFIER).is_err() {
        ks.put(JWT_IDENTIFIER.to_owned(), generate_priv_key())
            .unwrap();
    }
    let keystore = Arc::new(RwLock::new(ks));

    // Initialize database
    let mut db = RocksDb::new(config.data_dir + "/db");
    db.open().unwrap();
    let db = Arc::new(db);

    // Initialize StateManager
    let chain_store = Arc::new(ChainStore::new(Arc::clone(&db)));
    let state_manager = Arc::new(StateManager::new(Arc::clone(&chain_store)));

    // Sync from snapshot
    if let Some(path) = &config.snapshot_path {
        let file = File::open(path).expect("Snapshot file path not found!");
        let reader = BufReader::new(file);
        import_chain::<FullVerifier, _, _>(&state_manager, reader, false)
            .await
            .unwrap();
    }

    // Read Genesis file
    let (genesis, network_name) =
        initialize_genesis(config.genesis_file.as_ref(), &state_manager).unwrap();

    // Fetch and ensure verification keys are downloaded
    get_params_default(SectorSizeOpt::Keys, false)
        .await
        .unwrap();

    // Initialize mpool
    let publisher = chain_store.publisher();
    let subscriber = publisher.write().await.subscribe();
    let provider = MpoolRpcProvider::new(subscriber, Arc::clone(&state_manager));
    let mpool = Arc::new(
        MessagePool::new(
            provider,
            network_name.clone(),
            MpoolConfig::load_config(db.as_ref()).unwrap(),
        )
        .await
        .unwrap(),
    );

    // Libp2p service setup
    let p2p_service = Libp2pService::new(
        config.network,
        chain_store,
        Arc::clone(&mpool),
        net_keypair,
        &network_name,
    );
    let network_rx = p2p_service.network_receiver();
    let network_send = p2p_service.network_sender();

    // Get Drand Coefficients
    let coeff = config.drand_public;

    let beacon = DrandBeacon::new(
        DEFAULT_DRAND_URL,
        coeff,
        genesis.blocks()[0].timestamp(),
        EPOCH_DURATION_SECONDS as u64,
    )
    .await
    .unwrap();

    // Initialize ChainSyncer
    // TODO allow for configuring validation strategy (defaulting to full validation)
    let chain_syncer = ChainSyncer::<_, _, FullVerifier>::new(
        Arc::clone(&state_manager),
        Arc::new(beacon),
        network_send.clone(),
        network_rx,
        Arc::new(genesis),
    )
    .unwrap();
    let bad_blocks = chain_syncer.bad_blocks_cloned();
    let sync_state = chain_syncer.sync_state_cloned();
    let sync_task = task::spawn(async {
        chain_syncer.start(WORKER_TASKS).await;
    });

    // Start services
    let p2p_task = task::spawn(async {
        p2p_service.run().await;
    });
    let rpc_task = if config.enable_rpc {
        let keystore_rpc = Arc::clone(&keystore);
        let rpc_listen = format!("127.0.0.1:{}", &config.rpc_port);
        Some(task::spawn(async move {
            info!("JSON RPC Endpoint at {}", &rpc_listen);
            start_rpc(
                RpcState {
                    state_manager,
                    keystore: keystore_rpc,
                    mpool,
                    bad_blocks,
                    sync_state,
                    network_send,
                    network_name,
                    events_pubsub: Arc::new(RwLock::new(Publisher::new(1000))),
                },
                &rpc_listen,
            )
            .await;
        }))
    } else {
        debug!("RPC disabled");
        None
    };

    // Block until ctrl-c is hit
    block_until_sigint().await;

    let keystore_write = task::spawn(async move {
        keystore.read().await.flush().unwrap();
    });

    // Cancel all async services
    p2p_task.cancel().await;
    sync_task.cancel().await;
    if let Some(task) = rpc_task {
        task.cancel().await;
    }
    keystore_write.await;

    info!("Forest finish shutdown");
}

#[cfg(test)]
mod test {
    use super::*;
    use db::MemoryDB;
    use std::fs::File;
    use std::io::BufReader;

    #[async_std::test]
    async fn import_snapshot_from_file() {
        let db = Arc::new(MemoryDB::default());
        let cs = Arc::new(ChainStore::new(db));
        let sm = Arc::new(StateManager::new(cs));
        let file = File::open("test_files/chain4.car").expect("Snapshot file path not found!");
        let reader = BufReader::new(file);
        import_chain::<FullVerifier, _, _>(&sm, reader, true)
            .await
            .expect("Failed to import chain");
    }
    #[async_std::test]
    async fn import_chain_from_file() {
        let db = Arc::new(MemoryDB::default());
        let cs = Arc::new(ChainStore::new(db));
        let sm = Arc::new(StateManager::new(cs));
        let file = File::open("test_files/chain4.car").expect("Snapshot file path not found!");
        let reader = BufReader::new(file);
        import_chain::<FullVerifier, _, _>(&sm, reader, false)
            .await
            .expect("Failed to import chain");
    }
}

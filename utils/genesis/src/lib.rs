// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use forest_blocks::{BlockHeader, Tipset, TipsetKeys};
use forest_chain::ChainStore;
use forest_db::rocks::RocksDb;
use forest_db::rocks_config::RocksDbConfig;
use forest_fil_types::verifier::ProofVerifier;
use forest_ipld_blockstore::{BlockStore, BlockStoreExt};
use forest_net_utils::FetchProgress;
use forest_state_manager::StateManager;
use fvm_ipld_car::{load_car, CarReader};
use log::{debug, info};
use rocksdb::SstFileWriter;
use std::io::Stdout;
use std::path::PathBuf;
use std::sync::Arc;
use std::time;
use tokio::fs::File;
use tokio::io::AsyncRead;
use tokio::io::BufReader;
use tokio_util::compat::TokioAsyncReadCompatExt;
use url::Url;

#[cfg(feature = "testing")]
pub const EXPORT_SR_40: &[u8] = std::include_bytes!("export40.car");

/// Uses an optional file path or the default genesis to parse the genesis and determine if
/// chain store has existing data for the given genesis.
pub async fn read_genesis_header<DB>(
    genesis_fp: Option<&String>,
    genesis_bytes: Option<&[u8]>,
    cs: &ChainStore<DB>,
) -> Result<Tipset, anyhow::Error>
where
    DB: BlockStore + Send + Sync + 'static,
{
    let genesis = match genesis_fp {
        Some(path) => {
            let file = File::open(path).await?;
            let reader = BufReader::new(file);
            process_car(reader, cs).await?
        }
        None => {
            debug!("No specified genesis in config. Using default genesis.");
            let genesis_bytes =
                genesis_bytes.ok_or_else(|| anyhow::anyhow!("No default genesis."))?;
            let reader = BufReader::<&[u8]>::new(genesis_bytes);
            process_car(reader, cs).await?
        }
    };

    info!("Initialized genesis: {}", genesis);
    Ok(Tipset::new(vec![genesis])?)
}

pub async fn get_network_name_from_genesis<BS>(
    genesis_ts: &Tipset,
    state_manager: &StateManager<BS>,
) -> Result<String, anyhow::Error>
where
    BS: BlockStore + Send + Sync + 'static,
{
    // the genesis tipset has just one block, so fetch it
    let genesis_header = genesis_ts.min_ticket_block();

    // Get network name from genesis state.
    let network_name = state_manager
        .get_network_name(genesis_header.state_root())
        .map_err(|e| anyhow::anyhow!("Failed to retrieve network name from genesis: {}", e))?;
    Ok(network_name)
}

pub async fn initialize_genesis<BS>(
    genesis_fp: Option<&String>,
    state_manager: &StateManager<BS>,
) -> Result<(Tipset, String), anyhow::Error>
where
    BS: BlockStore + Send + Sync + 'static,
{
    let genesis_bytes = state_manager.chain_config().genesis_bytes();
    let ts = read_genesis_header(genesis_fp, genesis_bytes, state_manager.chain_store()).await?;
    let network_name = get_network_name_from_genesis(&ts, state_manager).await?;
    Ok((ts, network_name))
}

async fn process_car<R, BS>(
    reader: R,
    chain_store: &ChainStore<BS>,
) -> Result<BlockHeader, anyhow::Error>
where
    R: AsyncRead + Send + Unpin,
    BS: BlockStore + Send + Sync + 'static,
{
    // Load genesis state into the database and get the Cid
    let genesis_cids: Vec<Cid> = load_car(chain_store.blockstore(), reader.compat()).await?;
    if genesis_cids.len() != 1 {
        panic!("Invalid Genesis. Genesis Tipset must have only 1 Block.");
    }

    let genesis_block: BlockHeader =
        chain_store.db.get_obj(&genesis_cids[0])?.ok_or_else(|| {
            anyhow::anyhow!(
                "Could not find genesis block despite being loaded using a genesis file"
            )
        })?;

    let store_genesis = chain_store.genesis()?;

    if store_genesis
        .map(|store| store == genesis_block)
        .unwrap_or_default()
    {
        debug!("Genesis from config matches Genesis from store");
    } else {
        debug!("Initialize ChainSyncer with new genesis from config");
        chain_store.set_genesis(&genesis_block)?;

        chain_store
            .set_heaviest_tipset(Arc::new(Tipset::new(vec![genesis_block.clone()])?))
            .await?;
    }
    Ok(genesis_block)
}

/// Import a chain from a CAR file. If the snapshot boolean is set, it will not verify the chain
/// state and instead accept the largest height as genesis.
pub async fn import_chain<V: ProofVerifier, DB>(
    sm: &Arc<StateManager<DB>>,
    path: &str,
    validate_height: Option<i64>,
    skip_load: bool,
    rocksdb_config: &RocksDbConfig,
) -> Result<(), anyhow::Error>
where
    DB: BlockStore + Send + Sync + 'static,
{
    const SST_INGESTION: bool = false;
    let is_remote_file: bool = path.starts_with("http://") || path.starts_with("https://");

    info!("Importing chain from snapshot at: {path}");
    // start import
    let cids = if is_remote_file {
        info!("Downloading file...");
        let url = Url::parse(path)?;
        let reader = FetchProgress::fetch_from_url(url).await?;
        load_and_retrieve_header(sm.blockstore(), reader, skip_load).await?
    } else {
        if SST_INGESTION {
            info!("Reading file...");
            let file = File::open(&path).await?;
            let reader = FetchProgress::fetch_from_file(file).await?;
            let stopwatch = time::Instant::now();
            let mut compat = reader.compat();
            let mut car_reader = CarReader::new(&mut compat).await?;

            const TARGET_SIZE: usize = 256 * 1024 * 1024; // 256MB
            let mut buffer = Vec::with_capacity(4096);
            let mut size = 0;
            let mut id = 0;
            let mut paths: Vec<PathBuf> = vec![];
            while let Some(block) = car_reader.next_block().await.unwrap() {
                size += std::mem::size_of::<Cid>() + block.data.len();
                buffer.push((block.cid, block.data));
                if size >= TARGET_SIZE {
                    buffer.sort_by_key(|(k, _)| *k);

                    // Create sst writer
                    let opts = RocksDb::to_options(rocksdb_config);
                    let mut writer = SstFileWriter::create(&opts);

                    let mut sst_path: PathBuf = PathBuf::new();
                    sst_path.push(format!("file{id}.sst"));
                    paths.push(sst_path.clone());

                    writer.open(sst_path).unwrap();
                    for (k, v) in buffer.iter() {
                        writer.put(k.to_bytes(), v).unwrap();
                    }
                    writer.finish().unwrap();

                    buffer.clear();
                    size = 0;
                    id += 1;
                }
            }
            info!("Loaded .car file in {}s", stopwatch.elapsed().as_secs());
            sm.blockstore().ingest_sst_files(paths)?;

            car_reader.header.roots
        } else {
            info!("Reading file...");
            let file = File::open(&path).await?;
            let reader = FetchProgress::fetch_from_file(file).await?;
            {
                let stopwatch = time::Instant::now();
                sm.blockstore().begin_import()?;
                let cids = load_and_retrieve_header(sm.blockstore(), reader, skip_load).await?;
                info!("Loaded .car file in {}s", stopwatch.elapsed().as_secs());
                sm.blockstore().end_import()?;
                cids
            }
        }
    };
    let ts = sm
        .chain_store()
        .tipset_from_keys(&TipsetKeys::new(cids))
        .await?;

    if !skip_load {
        let gb = sm
            .chain_store()
            .tipset_by_height(0, ts.clone(), true)
            .await?;
        sm.chain_store().set_genesis(&gb.blocks()[0])?;
    }

    // Update head with snapshot header tipset
    sm.chain_store().set_heaviest_tipset(ts.clone()).await?;

    if let Some(height) = validate_height {
        info!("Validating imported chain from height: {}", height);
        sm.validate_chain::<V>(ts.clone(), height).await?;
    }

    info!("Accepting {:?} as new head.", ts.cids(),);
    Ok(())
}

/// Loads car file into database, and returns the block header CIDs from the CAR header.
async fn load_and_retrieve_header<DB, R>(
    store: &DB,
    reader: FetchProgress<R, Stdout>,
    skip_load: bool,
) -> Result<Vec<Cid>, anyhow::Error>
where
    DB: BlockStore,
    R: AsyncRead + Send + Unpin,
{
    let mut compat = reader.compat();
    let result = if skip_load {
        CarReader::new(&mut compat).await?.header.roots
    } else {
        load_car(store, &mut compat).await?
    };
    compat.into_inner().finish();
    Ok(result)
}

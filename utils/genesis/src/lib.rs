// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::bail;
use cid::Cid;
use forest_blocks::{BlockHeader, Tipset, TipsetKeys};
use forest_chain::ChainStore;
use forest_db::Store;
use forest_state_manager::StateManager;
use forest_utils::db::BlockstoreExt;
use forest_utils::net::FetchProgress;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_car::{load_car, CarReader};
use log::{debug, info};
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
    DB: Blockstore + Store + Send + Sync,
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

pub fn get_network_name_from_genesis<BS>(
    genesis_ts: &Tipset,
    state_manager: &StateManager<BS>,
) -> Result<String, anyhow::Error>
where
    BS: Blockstore + Store + Clone + Send + Sync + 'static,
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
    BS: Blockstore + Store + Clone + Send + Sync + 'static,
{
    let genesis_bytes = state_manager.chain_config().genesis_bytes();
    let ts = read_genesis_header(genesis_fp, genesis_bytes, state_manager.chain_store()).await?;
    let network_name = get_network_name_from_genesis(&ts, state_manager)?;
    Ok((ts, network_name))
}

async fn process_car<R, BS>(
    reader: R,
    chain_store: &ChainStore<BS>,
) -> Result<BlockHeader, anyhow::Error>
where
    R: AsyncRead + Send + Unpin,
    BS: Blockstore + Store + Send + Sync,
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

        chain_store.set_heaviest_tipset(Arc::new(Tipset::new(vec![genesis_block.clone()])?))?;
    }
    Ok(genesis_block)
}

/// Import a chain from a CAR file. If the snapshot boolean is set, it will not verify the chain
/// state and instead accept the largest height as genesis.
pub async fn import_chain<DB>(
    sm: &Arc<StateManager<DB>>,
    path: &str,
    validate_height: Option<i64>,
    skip_load: bool,
) -> Result<(), anyhow::Error>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
{
    let is_remote_file: bool = path.starts_with("http://") || path.starts_with("https://");

    info!("Importing chain from snapshot at: {path}");
    // start import
    let stopwatch = time::Instant::now();
    let cids = if is_remote_file {
        info!("Downloading file...");
        let url = Url::parse(path)?;
        let reader = FetchProgress::fetch_from_url(url).await?;
        load_and_retrieve_header(sm.blockstore(), reader, skip_load).await?
    } else {
        info!("Reading file...");
        let file = File::open(&path).await?;
        let reader = FetchProgress::fetch_from_file(file).await?;
        load_and_retrieve_header(sm.blockstore(), reader, skip_load).await?
    };

    info!("Loaded .car file in {}s", stopwatch.elapsed().as_secs());
    let ts = sm.chain_store().tipset_from_keys(&TipsetKeys::new(cids))?;

    if !skip_load {
        let gb = sm.chain_store().tipset_by_height(0, ts.clone(), true)?;
        sm.chain_store().set_genesis(&gb.blocks()[0])?;
        if !matches!(&sm.chain_config().genesis_cid, Some(expected_cid) if expected_cid ==  &gb.blocks()[0].cid().to_string())
        {
            bail!(
                "Snapshot incompatible with {}. Consider specifying the network with `--chain` flag or 
                 use a custom config file to set expected genesis CID for selected network", 
                sm.chain_config().name
            );
        }
    }

    // Update head with snapshot header tipset
    sm.chain_store().set_heaviest_tipset(ts.clone())?;

    sm.blockstore().flush()?;

    if let Some(height) = validate_height {
        let height = if height > 0 {
            height
        } else {
            (ts.epoch() + height).max(0)
        };
        info!("Validating imported chain from height: {}", height);
        sm.validate_chain(ts.clone(), height).await?;
    }

    info!("Accepting {:?} as new head.", ts.cids());

    Ok(())
}

/// Loads car file into database, and returns the block header CIDs from the CAR header.
async fn load_and_retrieve_header<DB, R>(
    store: &DB,
    reader: FetchProgress<R>,
    skip_load: bool,
) -> Result<Vec<Cid>, anyhow::Error>
where
    DB: Blockstore,
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

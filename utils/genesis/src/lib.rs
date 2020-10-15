// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blocks::{BlockHeader, Tipset};
use chain::ChainStore;
use cid::Cid;
use forest_car::load_car;
use ipld_blockstore::BlockStore;
use log::{debug, info};
use state_manager::StateManager;
use std::error::Error as StdError;
use std::fs::File;
use std::include_bytes;
use std::io::{BufReader, Read};
use std::sync::Arc;

#[cfg(feature = "testing")]
pub const EXPORT_SR_40: &[u8; 1226395] = include_bytes!("mainnet/export40.car");

/// Uses an optional file path or the default genesis to parse the genesis and determine if
/// chain store has existing data for the given genesis.
pub fn initialize_genesis<BS>(
    genesis_fp: Option<&String>,
    chain_store: &mut ChainStore<BS>,
    state_manager: &StateManager<BS>,
) -> Result<(Tipset, String), Box<dyn StdError>>
where
    BS: BlockStore,
{
    let genesis = match genesis_fp {
        Some(path) => {
            let file = File::open(path)?;
            let reader = BufReader::new(file);
            process_car(reader, chain_store)?
        }
        None => {
            debug!("No specified genesis in config. Using default genesis.");
            let bz = include_bytes!("mainnet/genesis.car");
            let reader = BufReader::<&[u8]>::new(bz.as_ref());
            process_car(reader, chain_store)?
        }
    };

    info!("Initialized genesis: {}", genesis);

    // Get network name from genesis state.
    let network_name = state_manager
        .get_network_name(genesis.state_root())
        .map_err(|e| format!("Failed to retrieve network name from genesis: {}", e))?;
    Ok((Tipset::new(vec![genesis])?, network_name))
}

fn process_car<R, BS>(
    reader: R,
    chain_store: &mut ChainStore<BS>,
) -> Result<BlockHeader, Box<dyn StdError>>
where
    R: Read,
    BS: BlockStore,
{
    // Load genesis state into the database and get the Cid
    let genesis_cids: Vec<Cid> = load_car(chain_store.blockstore(), reader)?;
    if genesis_cids.len() != 1 {
        panic!("Invalid Genesis. Genesis Tipset must have only 1 Block.");
    }

    let genesis_block: BlockHeader = chain_store.db.get(&genesis_cids[0])?.ok_or_else(|| {
        "Could not find genesis block despite being loaded using a genesis file".to_owned()
    })?;

    let store_genesis = chain_store.genesis()?;

    if store_genesis
        .map(|store| store == genesis_block)
        .unwrap_or_default()
    {
        debug!("Genesis from config matches Genesis from store");
        Ok(genesis_block)
    } else {
        debug!("Initialize ChainSyncer with new genesis from config");
        chain_store.set_genesis(&genesis_block)?;
        async_std::task::block_on(
            chain_store.set_heaviest_tipset(Arc::new(Tipset::new(vec![genesis_block.clone()])?)),
        )?;
        Ok(genesis_block)
    }
}

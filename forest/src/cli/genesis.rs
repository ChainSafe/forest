// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blocks::BlockHeader;
use blocks::Tipset;
use chain::ChainStore;
use cid::Cid;
use forest_car::load_car;
use ipld_blockstore::BlockStore;
use log::{debug, info};
use state_manager::StateManager;
use std::error::Error as StdError;
use std::fs::File;
use std::include_bytes;
use std::io::BufReader;
use std::sync::Arc;

/// Uses an optional file path or the default genesis to parse the genesis and determine if
/// chain store has existing data for the given genesis.
pub fn initialize_genesis<BS>(
    genesis_fp: &Option<String>,
    chain_store: &mut ChainStore<BS>,
) -> Result<(Tipset, String), Box<dyn StdError>>
where
    BS: BlockStore,
{
    let genesis = match genesis_fp {
        Some(path) => {
            let file = File::open(path).expect("Could not open genesis file");
            let reader = BufReader::new(file);
            process_car(reader, chain_store)?
        }
        None => {
            debug!("No specified genesis in config. Using default genesis.");
            let bz = include_bytes!("devnet.car");
            let reader = BufReader::new(bz.as_ref());
            process_car(reader, chain_store)?
        }
    };

    info!("Initialized genesis: {}", genesis);

    // This is just a workaround to get the network name before the sync process starts to use in
    // the pubsub topics, hopefully can be removed in future.
    let sm = StateManager::new(chain_store.db.clone());
    let network_name = sm.get_network_name(genesis.state_root()).expect(
        "Genesis not initialized properly, failed to retrieve network name. \
            Requires either a previously initialized genesis or with genesis config option set",
    );

    Ok((Tipset::new(vec![genesis])?, network_name))
}

fn process_car<R, BS>(
    reader: BufReader<R>,
    chain_store: &mut ChainStore<BS>,
) -> Result<BlockHeader, Box<dyn StdError>>
where
    R: std::io::Read,
    BS: BlockStore,
{
    // Load genesis state into the database and get the Cid
    let genesis_cids: Vec<Cid> = load_car(chain_store.blockstore(), reader).unwrap();
    if genesis_cids.len() != 1 {
        panic!("Invalid Genesis. Genesis Tipset must have only 1 Block.");
    }

    let genesis_block: BlockHeader = chain_store.db.get(&genesis_cids[0])?.ok_or_else(|| {
        "Could not find genesis block despite being loaded using a genesis file".to_owned()
    })?;

    let store_genesis = chain_store.genesis()?;

    if store_genesis.is_some() && store_genesis.unwrap() == genesis_block {
        debug!("Genesis from config matches Genesis from store");
        Ok(genesis_block)
    } else {
        debug!("Initialize ChainSyncer with new genesis from config");
        chain_store.set_genesis(genesis_block.clone())?;
        chain_store.set_heaviest_tipset(Arc::new(Tipset::new(vec![genesis_block.clone()])?))?;
        Ok(genesis_block)
    }
}

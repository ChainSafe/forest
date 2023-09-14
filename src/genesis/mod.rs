// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::BlockHeader;
use crate::state_manager::StateManager;
use crate::utils::db::car_util::load_car;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;

use tokio::{fs::File, io::BufReader};
use tracing::{debug, info};

#[cfg(test)]
pub const EXPORT_SR_40: &[u8] = std::include_bytes!("export40.car");

/// Uses an optional file path or the default genesis to parse the genesis and
/// determine if chain store has existing data for the given genesis.
pub async fn read_genesis_header<DB>(
    genesis_fp: Option<&String>,
    genesis_bytes: Option<&[u8]>,
    db: &DB,
) -> Result<BlockHeader, anyhow::Error>
where
    DB: Blockstore,
{
    let genesis = match genesis_fp {
        Some(path) => {
            let file = File::open(path).await?;
            let reader = BufReader::new(file);
            process_car(reader, db).await?
        }
        None => {
            debug!("No specified genesis in config. Using default genesis.");
            let genesis_bytes =
                genesis_bytes.ok_or_else(|| anyhow::anyhow!("No default genesis."))?;
            let reader = std::io::Cursor::new(genesis_bytes);
            process_car(reader, db).await?
        }
    };

    info!("Initialized genesis: {}", genesis);
    Ok(genesis)
}

pub fn get_network_name_from_genesis<BS>(
    genesis_header: &BlockHeader,
    state_manager: &StateManager<BS>,
) -> Result<String, anyhow::Error>
where
    BS: Blockstore,
{
    // Get network name from genesis state.
    let network_name = state_manager
        .get_network_name(genesis_header.state_root())
        .map_err(|e| anyhow::anyhow!("Failed to retrieve network name from genesis: {}", e))?;
    Ok(network_name)
}

async fn process_car<R, BS>(reader: R, db: &BS) -> Result<BlockHeader, anyhow::Error>
where
    R: tokio::io::AsyncBufRead + tokio::io::AsyncSeek + Send + Unpin,
    BS: Blockstore,
{
    // Load genesis state into the database and get the Cid
    let genesis_cids: Vec<Cid> = load_car(db, reader).await?;
    if genesis_cids.len() != 1 {
        panic!("Invalid Genesis. Genesis Tipset must have only 1 Block.");
    }

    let genesis_block = BlockHeader::load(db, genesis_cids[0])?.ok_or_else(|| {
        anyhow::anyhow!("Could not find genesis block despite being loaded using a genesis file")
    })?;

    Ok(genesis_block)
}

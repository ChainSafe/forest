// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::Path;

use crate::blocks::CachingBlockHeader;
use crate::state_manager::StateManager;
use crate::utils::db::car_util::load_car;
use anyhow::Context as _;
use fvm_ipld_blockstore::Blockstore;
use tokio::{
    fs::File,
    io::{AsyncBufRead, BufReader},
};
use tracing::{debug, info};

#[cfg(test)]
pub const EXPORT_SR_40: &[u8] = std::include_bytes!("export40.car");

/// Uses an optional file path or the default genesis to parse the genesis and
/// determine if chain store has existing data for the given genesis.
pub async fn read_genesis_header<DB>(
    genesis_fp: Option<&Path>,
    genesis_bytes: Option<&[u8]>,
    db: &DB,
) -> Result<CachingBlockHeader, anyhow::Error>
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
            let genesis_bytes = genesis_bytes.context("No default genesis.")?;
            process_car(genesis_bytes, db).await?
        }
    };

    info!("Initialized genesis: {}", genesis.cid());
    Ok(genesis)
}

pub fn get_network_name_from_genesis<BS>(
    genesis_header: &CachingBlockHeader,
    state_manager: &StateManager<BS>,
) -> Result<String, anyhow::Error>
where
    BS: Blockstore,
{
    // Get network name from genesis state.
    let network_name = state_manager
        .get_network_name(&genesis_header.state_root)
        .map_err(|e| anyhow::anyhow!("Failed to retrieve network name from genesis: {}", e))?;
    Ok(network_name)
}

async fn process_car<R, BS>(reader: R, db: &BS) -> Result<CachingBlockHeader, anyhow::Error>
where
    R: AsyncBufRead + Unpin,
    BS: Blockstore,
{
    // Load genesis state into the database and get the Cid
    let header = load_car(db, reader).await?;
    assert_eq!(
        header.roots.len(),
        1,
        "Invalid Genesis. Genesis Tipset must have only 1 Block."
    );

    let genesis_block = CachingBlockHeader::load(db, *header.roots.first())?.ok_or_else(|| {
        anyhow::anyhow!("Could not find genesis block despite being loaded using a genesis file")
    })?;

    Ok(genesis_block)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cid::Cid;

    #[tokio::test]
    async fn test_process_car_genesis_mainnet() {
        use crate::networks::mainnet::{DEFAULT_GENESIS, GENESIS_CID};

        let header = load_header_from_car(DEFAULT_GENESIS).await;
        assert_eq!(header.cid(), &GENESIS_CID as &Cid);
    }

    #[tokio::test]
    async fn test_process_car_genesis_calibnet() {
        use crate::networks::calibnet::{DEFAULT_GENESIS, GENESIS_CID};

        let header = load_header_from_car(DEFAULT_GENESIS).await;
        assert_eq!(header.cid(), &GENESIS_CID as &Cid);
    }

    async fn load_header_from_car(genesis_bytes: &[u8]) -> CachingBlockHeader {
        let db = crate::db::MemoryDB::default();
        process_car(genesis_bytes, &db).await.unwrap()
    }
}

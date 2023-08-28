// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{sync::Arc, time};

use crate::blocks::{BlockHeader, TipsetKeys};
use crate::chain::index::ResolveNullTipset;
use crate::cli_shared::cli::{BufferSize, ChunkSize};
use crate::state_manager::StateManager;
use crate::utils::net;
use anyhow::bail;
use cid::Cid;
use futures::{sink::SinkExt, stream, AsyncRead, Stream, StreamExt};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_car::{load_car, CarReader};

use tokio::{fs::File, io::BufReader};
use tokio_util::compat::TokioAsyncReadCompatExt;
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
            process_car(reader.compat(), db).await?
        }
        None => {
            debug!("No specified genesis in config. Using default genesis.");
            let genesis_bytes =
                genesis_bytes.ok_or_else(|| anyhow::anyhow!("No default genesis."))?;
            let reader = BufReader::<&[u8]>::new(genesis_bytes);
            process_car(reader.compat(), db).await?
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
    R: AsyncRead + Send + Unpin,
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

/// Import a chain from a CAR file. If the snapshot boolean is set, it will not
/// verify the chain state and instead accept the largest height as genesis.
pub async fn import_chain<DB>(
    sm: &Arc<StateManager<DB>>,
    path: &str,
    skip_load: bool,
    chunk_size: ChunkSize,
    buffer_size: BufferSize,
) -> anyhow::Result<()>
where
    DB: Blockstore + Send + Sync + 'static,
{
    info!("Importing chain from snapshot at: {path}");
    // start import
    let stopwatch = time::Instant::now();
    let reader = net::decompress_if_needed(net::reader(path).await?).await?;

    let (cids, n_records) = load_and_retrieve_header(
        sm.blockstore_owned(),
        reader.compat(),
        skip_load,
        chunk_size,
        buffer_size,
    )
    .await?;

    info!(
        "Loaded {} records from .car file in {}s",
        n_records.unwrap_or_default(),
        stopwatch.elapsed().as_secs()
    );
    if let Some(n_records) = n_records {
        sm.chain_store().set_estimated_records(n_records as u64)?;
    }

    let ts = sm.chain_store().tipset_from_keys(&TipsetKeys::from(cids))?;

    if !skip_load {
        let gb = sm.chain_store().chain_index.tipset_by_height(
            0,
            ts.clone(),
            ResolveNullTipset::TakeOlder,
        )?;
        if sm.chain_config().genesis_cid.is_some()
            && !matches!(&sm.chain_config().genesis_cid, Some(expected_cid) if expected_cid ==  &gb.blocks()[0].cid().to_string())
        {
            bail!(
                "Snapshot incompatible with {}. Consider specifying the network with `--chain` flag or \
                 use a custom config file to set expected genesis CID for selected network", 
                sm.chain_config().network
            );
        }
    }

    // Update head with snapshot header tipset
    info!("Accepting {:?} as new head.", ts.cids());
    sm.chain_store().set_heaviest_tipset(ts)?;

    Ok(())
}

/// Loads car file into database, and returns the block header CIDs from the CAR
/// header.
async fn load_and_retrieve_header<DB, R>(
    store: DB,
    reader: R,
    skip_load: bool,
    chunk_size: ChunkSize,
    buffer_size: BufferSize,
) -> anyhow::Result<(Vec<Cid>, Option<usize>)>
where
    DB: Blockstore + Send + 'static,
    R: AsyncRead + Send + Unpin,
{
    let result = if skip_load {
        (CarReader::new(reader).await?.header.roots, None)
    } else {
        let (roots, n_records) = forest_load_car(store, reader, chunk_size, buffer_size).await?;
        (roots, Some(n_records))
    };

    Ok(result)
}

fn car_stream<R: futures::AsyncRead + Send + Unpin>(
    reader: CarReader<R>,
) -> impl Stream<Item = anyhow::Result<fvm_ipld_car::Block>> {
    stream::unfold(reader, |mut reader| async move {
        reader
            .next_block()
            .await
            .map_err(anyhow::Error::from)
            .transpose()
            .map(|result| (result, reader))
    })
}

pub async fn forest_load_car<DB, R>(
    store: DB,
    reader: R,
    ChunkSize(chunk_size): ChunkSize,
    BufferSize(buffer_size): BufferSize,
) -> anyhow::Result<(Vec<Cid>, usize)>
where
    R: futures::AsyncRead + Send + Unpin,
    DB: Blockstore + Send + 'static,
{
    let mut car_reader = CarReader::new(reader).await?;
    let header = std::mem::take(&mut car_reader.header);
    let mut n_records = 0;

    let sink = futures::sink::unfold(
        store,
        |store, blocks: Vec<fvm_ipld_car::Block>| async move {
            tokio::task::spawn_blocking(move || {
                store.put_many_keyed(blocks.into_iter().map(|block| (block.cid, block.data)))?;
                Ok(store)
            })
            .await?
        },
    );

    // Stream key-value pairs from the CAR file and commit them in chunks. Try
    // to maintain a buffer of a few chunks to avoid read-stalling.
    car_stream(car_reader)
        .inspect(|_| n_records += 1)
        .chunks(chunk_size as usize)
        .map(|vec| vec.into_iter().collect::<anyhow::Result<Vec<_>>>())
        .forward(sink.buffer(buffer_size as usize))
        .await?;

    Ok((header.roots, n_records))
}

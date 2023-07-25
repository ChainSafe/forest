// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::TipsetKeys;
use crate::car_backed_blockstore::UncompressedCarV1BackedBlockstore;
use crate::chain::index::ResolveNullTipset;
use crate::chain::ChainStore;
use crate::db::db_engine::db_root;
use crate::db::db_engine::open_proxy_db;
use crate::genesis::read_genesis_header;
use crate::json::cid::CidJson;
use crate::rpc_client::state_ops::state_fetch_root;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use crate::state_manager::StateManager;
use crate::state_manager::NO_CALLBACK;
use crate::statediff::print_state_diff;
use anyhow::Context;
use cid::Cid;
use clap::Subcommand;
use serde_tuple::{self, Deserialize_tuple, Serialize_tuple};
use std::{path::Path, path::PathBuf, sync::Arc};
use tempfile::TempDir;

use super::handle_rpc_err;
use super::Config;

#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug)]
struct VestingSchedule {
    entries: Vec<VestingScheduleEntry>,
}

#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug)]
struct VestingScheduleEntry {
    epoch: ChainEpoch,
    amount: TokenAmount,
}

#[derive(Debug, Subcommand)]
pub enum StateCommands {
    Fetch {
        root: Cid,
        /// The `.car` file path to save the state root
        #[arg(short, long)]
        save_to_file: Option<PathBuf>,
    },
    Diff {
        /// The previous CID state root
        pre: Cid,
        /// The post CID state root
        post: Cid,
        /// The depth at which IPLD links are resolved
        #[arg(short, long)]
        depth: Option<u64>,
    },
    ComputeState {
        /// Path to a snapshot (.car files only)
        #[arg(long)]
        snapshot: PathBuf,
        /// Set the height that the VM will see
        #[arg(long)]
        vm_height: ChainEpoch,
        /// Generate json output
        #[arg(long)]
        json: bool,
    },
}

async fn print_computed_state(
    config: Config,
    snapshot: &Path,
    vm_height: ChainEpoch,
    json: bool,
) -> anyhow::Result<()> {
    println!("Computing state @{}", vm_height);

    println!("Network: {}", config.chain.network);

    let temp_dir = TempDir::new()?;
    println!("Using temp dir: {:?}", temp_dir.path());

    // Initialize UncompressedCarV1BackedBlockstore
    println!("Loading snapshot...");
    let reader = std::fs::File::open(snapshot)?;
    let store = Arc::new(
        UncompressedCarV1BackedBlockstore::new(reader)
            .context("couldn't read input CAR file - is it compressed?")?,
    );

    let tsk = TipsetKeys::new(store.roots());

    let genesis_header = read_genesis_header(
        config.client.genesis_file.as_ref(),
        config.chain.genesis_bytes(),
        &store,
    )
    .await?;

    // Initialize ChainStore
    let cs = Arc::new(ChainStore::new(
        store,
        config.chain.clone(),
        genesis_header,
        TempDir::new()?.path(),
    )?);

    // Initialize StateManager
    let sm = Arc::new(StateManager::new(cs.clone(), config.chain)?);

    let ts = sm.chain_store().tipset_from_keys(&tsk)?;

    let tipset = cs
        .chain_index
        .tipset_by_height(vm_height.into(), ts, ResolveNullTipset::TakeOlder)
        .context(format!("couldn't get a tipset at height {}", vm_height))?;

    if json {
        // call version with traces enabled
        let (_, trace_info) = sm.compute_tipset_state(tipset, NO_CALLBACK, true).await?;
        let json_trace = serde_json::to_string_pretty(&trace_info)?;
        println!("{}", json_trace);
    } else {
        let ((st, _), _) = sm.compute_tipset_state(tipset, NO_CALLBACK, false).await?;
        println!("computed state cid: {}", st);
    }

    Ok(())
}

impl StateCommands {
    pub async fn run(self, config: Config) -> anyhow::Result<()> {
        match self {
            Self::Fetch { root, save_to_file } => {
                println!(
                    "{}",
                    state_fetch_root((CidJson(root), save_to_file), &config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)?
                );
            }
            Self::Diff { pre, post, depth } => {
                let chain_path = config
                    .client
                    .data_dir
                    .join(config.chain.network.to_string());
                let blockstore = open_proxy_db(db_root(&chain_path), Default::default())?;

                if let Err(err) = print_state_diff(&blockstore, &pre, &post, depth) {
                    eprintln!("Failed to print state diff: {err}");
                }
            }
            Self::ComputeState {
                snapshot,
                vm_height,
                json,
            } => {
                print_computed_state(config, &snapshot, vm_height, json).await?;
            }
        }
        Ok(())
    }
}

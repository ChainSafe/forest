// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::TipsetKeys;
use crate::chain::index::ResolveNullTipset;
use crate::chain::ChainStore;
use crate::db::car::AnyCar;
use crate::db::db_engine::db_root;
use crate::db::db_engine::open_proxy_db;
use crate::genesis::read_genesis_header;
use crate::json::cid::CidJson;
use crate::networks::{calibnet, mainnet, ChainConfig, NetworkChain};
use crate::rpc_client::state_ops::state_fetch_root;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use crate::shim::machine::MultiEngine;
use crate::state_manager::apply_block_messages;
use crate::state_manager::NO_CALLBACK;
use crate::statediff::print_state_diff;
use anyhow::Context;
use cid::Cid;
use clap::Subcommand;
use serde_tuple::{self, Deserialize_tuple, Serialize_tuple};
use std::{path::PathBuf, sync::Arc};
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
        /// Generate JSON output
        #[arg(long)]
        json: bool,
    },
}

async fn print_computed_state(
    config: Config,
    snapshot: PathBuf,
    vm_height: ChainEpoch,
    json: bool,
) -> anyhow::Result<()> {
    // Initialize Blockstore
    let store = Arc::new(AnyCar::new(move || std::fs::File::open(&snapshot))?);

    let tsk = TipsetKeys::new(store.roots());

    // Initialize ChainStore
    let genesis_header = read_genesis_header(
        config.client.genesis_file.as_ref(),
        config.chain.genesis_bytes(),
        &store,
    )
    .await?;

    let cs = Arc::new(ChainStore::new(
        store.clone(),
        config.chain.clone(),
        genesis_header,
        TempDir::new()?.path(),
    )?);

    // Prepare call to apply_block_messages
    let ts = cs.tipset_from_keys(&tsk)?;

    let genesis = ts.genesis(&store)?;
    let network = if genesis.cid() == &*calibnet::GENESIS_CID {
        NetworkChain::Calibnet
    } else if genesis.cid() == &*mainnet::GENESIS_CID {
        NetworkChain::Mainnet
    } else {
        NetworkChain::Devnet("devnet".to_string())
    };

    let chain_config = ChainConfig::from_chain(&network);

    let timestamp = cs.genesis().timestamp();
    let beacon = Arc::new(chain_config.get_beacon_schedule(timestamp));
    let tipset = cs
        .chain_index
        .tipset_by_height(vm_height, ts, ResolveNullTipset::TakeOlder)
        .context(format!("couldn't get a tipset at height {}", vm_height))?;

    let ((st, _), output) = apply_block_messages(
        timestamp,
        Arc::clone(&cs.chain_index),
        Arc::clone(&Arc::new(chain_config)),
        beacon,
        &MultiEngine::default(),
        tipset,
        NO_CALLBACK,
        json, // enable traces if json flag is used
    )?;

    if json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
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
                print_computed_state(config, snapshot, vm_height, json).await?;
            }
        }
        Ok(())
    }
}

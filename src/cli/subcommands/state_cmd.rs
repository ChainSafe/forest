// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::chain::ChainStore;
use crate::db::db_engine::db_root;
use crate::db::db_engine::open_proxy_db;
use crate::db::utils::parity::TempParityDB;
use crate::genesis::{import_chain, read_genesis_header};
use crate::json::cid::CidJson;
use crate::rpc_client::state_ops::state_fetch_root;
use crate::shim::clock::ChainEpoch;
use crate::state_manager::StateManager;
use crate::statediff::print_state_diff;
use anyhow::Context;
use cid::Cid;
use clap::Subcommand;
use fvm_shared::econ::TokenAmount;
use serde_tuple::{self, Deserialize_tuple, Serialize_tuple};
use std::{path::Path, path::PathBuf, sync::Arc};

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
        /// Path to a snapshot (.car or .car.zst)
        #[arg(long)]
        snapshot: PathBuf,
        /// Set the height that the VM will see
        #[arg(long)]
        vm_height: ChainEpoch,
        /// Message CID
        #[arg(long)]
        cid: Cid,
        /// Generate json output
        #[arg(long)]
        json: bool,
    },
}

async fn print_computed_state(
    config: Config,
    snapshot: &Path,
    vm_height: ChainEpoch,
    mcid: Cid,
    json: bool,
) -> anyhow::Result<()> {
    println!("Computing state @{}", vm_height);

    let temp = TempParityDB::new();

    println!("Network: {}", config.chain.network);

    // TODO: maybe check if there is a mismatch between snapshot and network
    let genesis_header = read_genesis_header(
        config.client.genesis_file.as_ref(),
        config.chain.genesis_bytes(),
        &temp.db,
    )
    .await?;

    println!("Using temp path: {:?}", temp.dir.path());

    // Initialize ChainStore
    let cs = Arc::new(ChainStore::new(
        temp.db,
        config.chain.clone(),
        &genesis_header,
        temp.dir.path(),
    )?);

    // Initialize StateManager
    let sm = Arc::new(StateManager::new(
        cs.clone(),
        config.chain,
        Arc::new(crate::interpreter::RewardActorMessageCalc),
    )?);
    import_chain::<_>(&sm, snapshot.to_str().unwrap(), false).await?;

    let heaviest = cs.heaviest_tipset();

    let tipset = cs
        .tipset_by_height(vm_height.into(), heaviest, false)
        .context(format!("couldn't get a tipset at height {}", vm_height))?;

    println!("Replaying message...");

    let (msg, ret) = sm.replay(&tipset, mcid).await?;

    println!("msg:\n{:?}", msg);
    println!("ret:\n{:?}", ret);

    Ok(())
}

impl StateCommands {
    pub async fn run(self, config: Config) -> anyhow::Result<()> {
        match self {
            Self::Fetch { root } => {
                println!(
                    "{}",
                    state_fetch_root((CidJson(root),), &config.client.rpc_token)
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
                cid,
                json,
            } => {
                print_computed_state(config, &snapshot, vm_height, cid, json).await?;
            }
        }
        Ok(())
    }
}

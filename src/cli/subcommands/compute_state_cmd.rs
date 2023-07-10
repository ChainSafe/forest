// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::chain::ChainStore;
use crate::db::utils::parity::TempParityDB;
use crate::genesis::{import_chain, read_genesis_header};
use crate::shim::clock::ChainEpoch;
use crate::state_manager::StateManager;
use anyhow::Context;
use cid::Cid;
use std::{path::PathBuf, sync::Arc};

//use super::handle_rpc_err;

use super::Config;

/// Perform state computations
#[derive(Debug, clap::Args)]
pub struct ComputeStateCommand {
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
}

impl ComputeStateCommand {
    pub async fn run(self, config: Config) -> anyhow::Result<()> {
        match self {
            _ => {
                println!("Computing state @{}", self.vm_height);

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
                import_chain::<_>(&sm, self.snapshot.to_str().unwrap(), false).await?;

                let heaviest = cs.heaviest_tipset();

                let tipset = cs
                    .tipset_by_height(self.vm_height.into(), heaviest, false)
                    .context(format!(
                        "couldn't get a tipset at height {}",
                        self.vm_height
                    ))?;

                //let reward_calc = cns::reward_calc();
                println!("Replaying message...");

                let (msg, ret) = sm.replay(&tipset, self.cid).await?;

                println!("msg:\n{:?}", msg);
                println!("ret:\n{:?}", ret);
            }
        }
        Ok(())
    }
}

// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::bigint::BigInt;
use log::{info, warn};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::str::FromStr;
use structopt::StructOpt;
use uuid::Uuid;

use forest_fil_types::genesis::{Actor, ActorType, Miner, Template as GenesisTemplate};
use fvm_shared::address::Address;
use fvm_shared::FILECOIN_PRECISION;

const ACCOUNT_START: u64 = 1000;
#[derive(Debug, StructOpt)]
pub enum GenesisCommands {
    /// Creates new genesis template
    NewTemplate {
        /// Input a network name
        #[structopt(short)]
        network_name: Option<String>,
        /// File path, i.e, `./genesis.json`. This command WILL NOT create a directory if it does not exist.
        #[structopt(short, default_value = "genesis.json")]
        file_path: String,
    },
    /// Add a miner to Genesis.
    AddMiner {
        /// Genesis file path
        #[structopt(short)]
        genesis_path: String,
        /// Pre-seal file path
        #[structopt(short)]
        preseal_path: String,
    },
}

impl GenesisCommands {
    pub async fn run(&self) {
        match self {
            Self::NewTemplate {
                network_name,
                file_path,
            } => {
                let template = GenesisTemplate::new(
                    network_name
                        .as_ref()
                        .unwrap_or(&format!("localnet-{}", Uuid::new_v4()))
                        .to_string(),
                );

                match &File::create(file_path) {
                    Ok(file) => {
                        serde_json::to_writer_pretty(file, &template).unwrap();
                    }
                    Err(err) => {
                        warn!("Can not write to a file, error: {}", err);
                    }
                }
            }
            Self::AddMiner {
                genesis_path,
                preseal_path,
            } => {
                if let Err(err) = add_miner(genesis_path.to_string(), preseal_path.to_string()) {
                    warn!("Cannot add miner(s), error: {}", err);
                };
            }
        }
    }
}

fn add_miner(genesis_path: String, preseal_path: String) -> Result<(), anyhow::Error> {
    let mut genesis_str = String::new();
    File::open(&genesis_path)?.read_to_string(&mut genesis_str)?;
    let mut template: GenesisTemplate = serde_json::from_str(&genesis_str)?;

    let mut preseal_str = String::new();
    File::open(preseal_path)?.read_to_string(&mut preseal_str)?;
    let miners: HashMap<String, Miner> = serde_json::from_str(&preseal_str)?;

    for (miner_address_str, miner) in miners.into_iter() {
        info!("Adding miner {} to genesis template", miner_address_str);

        let id = ACCOUNT_START + template.miners.len() as u64;

        let maddress = match Address::from_str(&miner_address_str) {
            Ok(addr) => addr,
            Err(e) => {
                info!("Can not parse miner's address {}: {}", miner_address_str, e);
                continue;
            }
        };

        let mid = maddress.id()?;

        if mid != id {
            info!("Tried to set miner {} as {}", mid, id);
            continue;
        }

        let miner_owner = miner.owner;
        template.miners.push(miner);

        info!("Giving {} some intial balance", miner_owner);
        template.accounts.push(Actor {
            actor_type: ActorType::Account,
            balance: BigInt::from(50_000_000) * FILECOIN_PRECISION,
            owner: miner_owner,
        })
    }

    serde_json::to_writer_pretty(
        OpenOptions::new().write(true).open(&genesis_path)?,
        &template,
    )?;

    Ok(())
}

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use address::{json::AddressJson, Address};
use blocks::tipset_keys_json::TipsetKeysJson;
use rpc_client::{chain_head, state_get_actor};
use structopt::StructOpt;

use super::handle_rpc_err;

#[derive(Debug, StructOpt)]
pub enum StateCommands {
    #[structopt(about = "Query network or miner power")]
    Power {
        #[structopt(about = "The miner address to query. Optional", short)]
        miner_address: Option<String>,
    },
    ListMiners,
    ListActors,
}

impl StateCommands {
    pub async fn run(&self) {
        match self {
            Self::Power { miner_address } => {
                let miner_address = miner_address.to_owned();

                match miner_address {
                    Some(miner_addr) => {
                        let address = Address::from_str(&miner_addr)
                            .expect(&format!("Cannot read address {}", miner_addr));
                        let tipset = chain_head().await.map_err(handle_rpc_err).unwrap();

                        let _actor_state = state_get_actor((
                            AddressJson(address),
                            TipsetKeysJson(tipset.0.key().to_owned()),
                        ));
                    }
                    None => {}
                }
            }
            Self::ListMiners => {}
            Self::ListActors => {}
        }
    }
}

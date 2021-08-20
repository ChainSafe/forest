// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use actor::{actorv3::ActorState, is_miner_actor};
use address::{json::AddressJson, Address};
use blocks::tipset_keys_json::TipsetKeysJson;
use num_bigint::BigInt;
use rpc_client::{chain_head, state_get_actor, state_miner_power};
use structopt::StructOpt;

use crate::cli::{cli_error_and_die, to_size_string};

use super::handle_rpc_err;

#[derive(Debug, StructOpt)]
pub enum StateCommands {
    #[structopt(about = "Query network or miner power")]
    Power {
        #[structopt(about = "The miner address to query. Optional", short)]
        miner_address: Option<String>,
    },
    #[structopt(about = "Retrieve miner information")]
    MinerInfo {
        #[structopt(about = "The address of the miner", short)]
        miner_address: String,
    },
    MarketBalance,
}

impl StateCommands {
    pub async fn run(&self) {
        match self {
            Self::Power { miner_address } => {
                let miner_address = miner_address.to_owned();

                let tipset = chain_head().await.map_err(handle_rpc_err).unwrap();
                let tipset_keys_json = TipsetKeysJson(tipset.0.key().to_owned());

                match miner_address {
                    Some(miner_addr) => {
                        let address = Address::from_str(&miner_addr)
                            .expect(&format!("Cannot read address {}", miner_addr));

                        match state_get_actor((AddressJson(address), tipset_keys_json.clone()))
                            .await
                            .map_err(handle_rpc_err)
                            .unwrap()
                        {
                            Some(actor_json) => {
                                let actor_state: ActorState = actor_json.into();
                                if !is_miner_actor(&actor_state.code) {
                                    cli_error_and_die(
                                        "Miner address does not correspond with a miner actor",
                                        1,
                                    );
                                }
                            }
                            None => cli_error_and_die(
                                &format!("cannot find miner at address {}", miner_addr),
                                1,
                            ),
                        };

                        let power = state_miner_power((
                            Some(
                                Address::from_str(&miner_addr)
                                    .expect("error: invalid address")
                                    .into(),
                            ),
                            tipset_keys_json,
                        ))
                        .await
                        .map_err(handle_rpc_err)
                        .unwrap();

                        let mp = power.miner_power;
                        let tp = power.total_power;

                        println!(
                            "{}({}) / {}({}) ~= {}%",
                            mp.quality_adj_power.to_string(),
                            to_size_string(&mp.quality_adj_power),
                            tp.quality_adj_power.to_string(),
                            to_size_string(&tp.quality_adj_power),
                            BigInt::from((mp.quality_adj_power * 100) / tp.quality_adj_power)
                                .to_string()
                        )
                    }
                    None => {
                        let power = state_miner_power((None, tipset_keys_json))
                            .await
                            .map_err(handle_rpc_err)
                            .unwrap();

                        let total_power = power.total_power;
                        println!(
                            "{}({})",
                            total_power.quality_adj_power.to_string(),
                            to_size_string(&total_power.quality_adj_power)
                        )
                    }
                }
            }
            Self::MinerInfo { miner_address } => {}
            Self::MarketBalance => {}
        }
    }
}

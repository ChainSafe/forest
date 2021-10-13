// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use actor::{actorv3::ActorState, is_miner_actor};
use address::{json::AddressJson, Address};
use blocks::{tipset_json::TipsetJson, tipset_keys_json::TipsetKeysJson};
use rpc_client::{
    chain_head, state_account_key, state_get_actor, state_list_actors, state_lookup,
    state_miner_power,
};
use structopt::StructOpt;

use crate::cli::{balance_to_fil, cli_error_and_die, to_size_string};

use super::handle_rpc_err;

#[derive(Debug, StructOpt)]
pub enum StateCommands {
    #[structopt(about = "Query miner power")]
    Power {
        #[structopt(about = "The miner address to query")]
        miner_address: String,
    },
    #[structopt(about = "Print actor information")]
    GetActor {
        #[structopt(about = "Address of actor to query")]
        address: String,
    },
    #[structopt(about = "List all actors on the network")]
    ListMiners,
    #[structopt(about = "Find corresponding ID address")]
    Lookup {
        #[structopt(short)]
        reverse: bool,
        #[structopt(about = "address")]
        address: String,
    },
}

impl StateCommands {
    pub async fn run(&self) {
        match self {
            Self::Power { miner_address } => {
                let miner_address = miner_address.to_owned();

                let tipset = chain_head().await.map_err(handle_rpc_err).unwrap();
                let tipset_keys_json = TipsetKeysJson(tipset.0.key().to_owned());

                let address = Address::from_str(&miner_address)
                    .unwrap_or_else(|_| panic!("Cannot read address {}", miner_address));

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
                        &format!("cannot find miner at address {}", miner_address),
                        1,
                    ),
                };

                let params = (
                    Some(
                        Address::from_str(&miner_address)
                            .expect("error: invalid address")
                            .into(),
                    ),
                    tipset_keys_json,
                );

                let power = state_miner_power(params)
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
                    (mp.quality_adj_power * 100) / tp.quality_adj_power
                );
            }
            Self::GetActor { address } => {
                let address = Address::from_str(&address.clone()).unwrap_or_else(|_| {
                    panic!("Failed to create address from argument {}", address)
                });

                let TipsetJson(tipset) = chain_head().await.map_err(handle_rpc_err).unwrap();

                let tsk = TipsetKeysJson(tipset.key().to_owned());

                let params = (AddressJson(address), tsk);

                let actor = state_get_actor(params)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();

                if let Some(state) = actor {
                    let a: ActorState = state.into();

                    println!("Address:\t{}", address);
                    println!(
                        "Balance:\t{:.23} FIL",
                        balance_to_fil(a.balance).expect("Couldn't convert balance to fil")
                    );
                    println!("Nonce:  \t{}", a.sequence);
                    println!("Code:   \t{}", a.code);
                } else {
                    println!("No information for actor found")
                }
            }
            Self::ListMiners => {
                let TipsetJson(tipset) = chain_head().await.map_err(handle_rpc_err).unwrap();
                let tsk = TipsetKeysJson(tipset.key().to_owned());

                let actors = state_list_actors((tsk,))
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();

                for a in actors {
                    let AddressJson(addr) = a;
                    println!("{}", addr.to_string());
                }
            }
            Self::Lookup { reverse, address } => {
                let address = Address::from_str(address)
                    .unwrap_or_else(|_| panic!("Invalid address: {}", address));

                let tipset = chain_head().await.map_err(handle_rpc_err).unwrap();

                let TipsetJson(ts) = tipset;

                let params = (AddressJson(address), TipsetKeysJson(ts.key().to_owned()));

                if !reverse {
                    match state_lookup(params).await.map_err(handle_rpc_err).unwrap() {
                        Some(AddressJson(addr)) => println!("{}", addr),
                        None => println!("No address found"),
                    };
                } else {
                    match state_account_key(params)
                        .await
                        .map_err(handle_rpc_err)
                        .unwrap()
                    {
                        Some(AddressJson(addr)) => {
                            println!("{}", addr)
                        }
                        None => println!("Nothing found"),
                    };
                }
            }
        }
    }
}

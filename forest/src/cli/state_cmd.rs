// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;
use rpc_client::chain_ops::*;
use forest_json::cid::CidJson;
use cid::Cid;
use fvm_ipld_encoding::{RawBytes};
use statediff::MinerState;
use forest_vm::TokenAmount;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::bigint::BigInt;
use fvm_shared::bigint::bigint_ser;
use fvm_ipld_encoding::tuple::*;
use num_traits::cast::FromPrimitive;
use num_rational::BigRational;
use std::ops::Shl;
use num_traits::ToPrimitive;
use bigdecimal::BigDecimal;

use actor_interface::is_miner_actor;
use forest_blocks::{tipset_json::TipsetJson, tipset_keys_json::TipsetKeysJson};
use forest_json::address::json::AddressJson;
use fvm::state_tree::ActorState;
use fvm_shared::address::Address;
use rpc_client::{
    chain_head, state_account_key, state_get_actor, state_list_actors, state_lookup,
    state_miner_power,
};
use structopt::StructOpt;

use crate::cli::{balance_to_fil, cli_error_and_die, to_size_string};

use super::handle_rpc_err;

#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug)]
struct VestingSchedule {
    data1: InnerVestingSchedule,
}

#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug)]
struct InnerVestingSchedule {
    data1: InnerInnerVestingSchedule,
    data2: InnerInnerVestingSchedule,
    data3: InnerInnerVestingSchedule,
    data4: InnerInnerVestingSchedule,
    data5: InnerInnerVestingSchedule,
    data6: InnerInnerVestingSchedule,
    data7: InnerInnerVestingSchedule,
    data8: InnerInnerVestingSchedule,
    data9: InnerInnerVestingSchedule,
    data10: InnerInnerVestingSchedule,
    data11: InnerInnerVestingSchedule,
    data12: InnerInnerVestingSchedule,
    data13: InnerInnerVestingSchedule,
    data14: InnerInnerVestingSchedule,
    data15: InnerInnerVestingSchedule,
    data16: InnerInnerVestingSchedule,
    data17: InnerInnerVestingSchedule,
    data18: InnerInnerVestingSchedule,
    data19: InnerInnerVestingSchedule,
    data20: InnerInnerVestingSchedule,
    data21: InnerInnerVestingSchedule,
    data22: InnerInnerVestingSchedule,
    data23: InnerInnerVestingSchedule,
    data24: InnerInnerVestingSchedule,
    data25: InnerInnerVestingSchedule,
    data26: InnerInnerVestingSchedule,
    data27: InnerInnerVestingSchedule,
    data28: InnerInnerVestingSchedule,
    data29: InnerInnerVestingSchedule,
    data30: InnerInnerVestingSchedule,
    data31: InnerInnerVestingSchedule,
}

#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug)]
struct InnerInnerVestingSchedule {
    data1: ChainEpoch,
    #[serde(with = "bigint_ser")]
    data2: TokenAmount,
}

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
    VestingTable {
        #[structopt(about = "Miner address to display vesting table")]
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
                    &mp.quality_adj_power,
                    to_size_string(&mp.quality_adj_power),
                    &tp.quality_adj_power,
                    to_size_string(&tp.quality_adj_power),
                    (&mp.quality_adj_power * 100) / &tp.quality_adj_power
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
                    println!("{}", addr);
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
            Self::VestingTable { address } => {
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
                
                let a: ActorState = actor.unwrap().into();
                let cid: Cid = a.state;

                let raw = chain_read_obj((CidJson(cid),)).await;
                let rawstring = match raw {
                    Err(_) => String::from(""),
                    Ok(raw) => raw,
                };
                //println!("{:?}", rawstring);
                let hex_decode_string = hex::decode(&rawstring).unwrap_or_else(|_| {
                    panic!("Failed to parse argument as hex")
                });
                let cornyname = RawBytes::from(hex_decode_string);
                //println!("{:?}", cornyname);
                let output: MinerState = RawBytes::deserialize(&cornyname).unwrap();
                //println!("{:?}", output.vesting_funds);

                let raw2 = chain_read_obj((CidJson(output.vesting_funds),)).await;
                let rawstring2 = match raw2 {
                    Err(_) => String::from(""),
                    Ok(raw2) => raw2,
                };
                //println!("{:?}", rawstring2);
                let hex_decode_string2 = hex::decode(&rawstring2).unwrap_or_else(|_| {
                    panic!("Failed to parse argument as hex")
                });
                let cornyname2 = RawBytes::from(hex_decode_string2);
                println!("{:?}", cornyname2);
                //let array_length = cornyname2[2];
                //println!("{:?}", array_length);
                //RawBytes::deserialize(&cornyname2).unwrap().();
                let output2: VestingSchedule = RawBytes::deserialize(&cornyname2).unwrap();
                //println!("{:?}", output2);
                println!("Vesting Schedule for Miner {}:", address);
                //println!("Epoch: {}     FIL: {:.3}", output2.data1.data1.data1, &output2.data1.data1.data2 / (BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data1.data1, BigDecimal::from(output2.data1.data1.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data2.data1, BigDecimal::from(output2.data1.data2.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data3.data1, BigDecimal::from(output2.data1.data3.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data4.data1, BigDecimal::from(output2.data1.data4.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data5.data1, BigDecimal::from(output2.data1.data5.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data6.data1, BigDecimal::from(output2.data1.data6.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data7.data1, BigDecimal::from(output2.data1.data7.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data8.data1, BigDecimal::from(output2.data1.data8.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data9.data1, BigDecimal::from(output2.data1.data9.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data10.data1, BigDecimal::from(output2.data1.data10.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data11.data1, BigDecimal::from(output2.data1.data11.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data12.data1, BigDecimal::from(output2.data1.data12.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data13.data1, BigDecimal::from(output2.data1.data13.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data14.data1, BigDecimal::from(output2.data1.data14.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data15.data1, BigDecimal::from(output2.data1.data15.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data16.data1, BigDecimal::from(output2.data1.data16.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data17.data1, BigDecimal::from(output2.data1.data17.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data18.data1, BigDecimal::from(output2.data1.data18.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data19.data1, BigDecimal::from(output2.data1.data19.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data20.data1, BigDecimal::from(output2.data1.data20.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data21.data1, BigDecimal::from(output2.data1.data21.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data22.data1, BigDecimal::from(output2.data1.data22.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data23.data1, BigDecimal::from(output2.data1.data23.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data24.data1, BigDecimal::from(output2.data1.data24.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data25.data1, BigDecimal::from(output2.data1.data25.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data26.data1, BigDecimal::from(output2.data1.data26.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data27.data1, BigDecimal::from(output2.data1.data27.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data28.data1, BigDecimal::from(output2.data1.data28.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data29.data1, BigDecimal::from(output2.data1.data29.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data30.data1, BigDecimal::from(output2.data1.data30.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                println!("Epoch: {}     FIL: {:.3}", output2.data1.data31.data1, BigDecimal::from(output2.data1.data31.data2) / BigDecimal::from(BigInt::from_f64(1e18).unwrap()));
                //println!("Epoch: {}     FIL: {}", output2.data1.data1.data1, output2.data1.data1.data2 / BigInt::from(1 << 18));
                //println!("{:?}", (BigInt::from_f64(1e18).unwrap()));
                //println!("Epoch: {}     FIL: {}", output2.data1.data1.data1, q128_to_f64(output2.data1.data1.data2));
                //println!("{:?}", output2.data1.data1.data1);
            }
        }
    }
}

// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Config;
use forest_blocks::tipset_keys_json::TipsetKeysJson;
use forest_json::address::json::AddressJson;
use forest_message::Message;
use forest_message::SignedMessage;
use fvm_shared::address::Address;
use fvm_shared::bigint::BigInt;
use jsonrpc_v2::Error;
use std::collections::HashMap;
use structopt::StructOpt;

use forest_json::cid::vec::CidJsonVec;
use forest_rpc_client::chain_ops::*;
use forest_rpc_client::mpool_ops::*;
use forest_rpc_client::state_ops::state_get_actor;
use forest_rpc_client::wallet_ops::wallet_list;

use crate::cli::handle_rpc_err;

#[derive(Debug, StructOpt)]
pub enum MpoolCommands {
    /// Get pending messages
    Pending,
    /// Print mempool stats
    Stat {
        /// Number of blocks to look back for minimum base fee
        #[structopt(short, default_value = "60")]
        base_fee_lookback: u32,
        /// Print stats for local addresses only
        #[structopt(short)]
        local: bool,
    },
}

impl MpoolCommands {
    pub async fn run(&self, config: Config) {
        match self {
            Self::Pending => {
                let res = mpool_pending((CidJsonVec(vec![]),), &config.client.rpc_token).await;
                let messages = res.map_err(handle_rpc_err).unwrap();
                println!("{:#?}", messages);
            }
            Self::Stat {
                base_fee_lookback,
                local,
            } => {
                let base_fee_lookback = *base_fee_lookback;
                let local = *local;

                let tipset_json = chain_head(&config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
                let tipset = tipset_json.0;

                let current_base_fee = tipset.blocks()[0].parent_base_fee().to_owned();
                let mut min_base_fee = current_base_fee.clone();

                let mut current_tipset = tipset.clone();

                for _ in 1..base_fee_lookback {
                    current_tipset = chain_get_tipset(
                        (current_tipset.parents().to_owned().into(),),
                        &config.client.rpc_token,
                    )
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap()
                    .0;

                    if current_tipset.blocks()[0].parent_base_fee() < &min_base_fee {
                        min_base_fee = current_tipset.blocks()[0].parent_base_fee().clone();
                    }

                    let wallet_response = wallet_list(&config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)
                        .unwrap();

                    let mut addresses = Vec::new();

                    if local {
                        addresses = wallet_response
                            .into_iter()
                            .map(|address| address.0)
                            .collect();
                    }

                    let messages = mpool_pending((CidJsonVec(vec![]),), &config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)
                        .unwrap();

                    struct StatBucket {
                        messages: HashMap<u64, SignedMessage>,
                    }

                    struct MpStat {
                        address: String,
                        past: u64,
                        current: u64,
                        future: u64,
                        below_current: u64,
                        below_past: u64,
                        gas_limit: BigInt,
                    }

                    let mut buckets = HashMap::<Address, StatBucket>::new();

                    for message in &messages {
                        if !addresses.iter().any(|&addr| addr == message.message().from) {
                            continue;
                        }

                        match buckets.get_mut(&message.message().from) {
                            Some(bucket) => {
                                bucket
                                    .messages
                                    .insert(message.message().sequence, message.to_owned());
                            }
                            None => {
                                buckets.insert(
                                    message.message().from,
                                    StatBucket {
                                        messages: HashMap::new(),
                                    },
                                );
                            }
                        };
                    }

                    let mut stats: Vec<MpStat> = Vec::new();

                    for (address, bucket) in buckets.iter() {
                        let get_actor_result = state_get_actor(
                            (
                                AddressJson(address.to_owned()),
                                TipsetKeysJson(tipset.key().to_owned()),
                            ),
                            &config.client.rpc_token,
                        )
                        .await;

                        let actor_json = match get_actor_result {
                            Ok(actor_json) => actor_json.unwrap(),
                            Err(err) => {
                                let error_message = match err {
                                    Error::Full { message, .. } => message,
                                    Error::Provided { message, .. } => message.to_string(),
                                };

                                println!("{}, err: {}", address, error_message);
                                continue;
                            }
                        };

                        let mut cur = actor_json.nonce();

                        while bucket.messages.get(&cur).is_some() {
                            cur += 1;
                        }

                        let mut stat = MpStat {
                            address: address.to_string(),
                            past: 0,
                            current: 0,
                            future: 0,
                            below_current: 0,
                            below_past: 0,
                            gas_limit: BigInt::from(0),
                        };

                        for message in messages.iter() {
                            if message.message().sequence < actor_json.nonce() {
                                stat.past += 1;
                            } else if message.message().sequence > cur {
                                stat.future += 1;
                            } else {
                                stat.current += 1;
                            }

                            if message.gas_fee_cap() < &current_base_fee {
                                stat.below_current += 1;
                            }

                            if message.gas_fee_cap() < &min_base_fee {
                                stat.below_past += 1;
                            }

                            stat.gas_limit += message.message().gas_limit;
                        }

                        stats.push(stat);
                    }

                    let mut total = MpStat {
                        address: String::new(),
                        past: 0,
                        current: 0,
                        future: 0,
                        below_current: 0,
                        below_past: 0,
                        gas_limit: BigInt::from(0),
                    };

                    for stat in stats {
                        total.past += stat.past;
                        total.current += stat.current;
                        total.future += stat.future;
                        total.below_current += stat.below_current;
                        total.below_past += stat.below_past;
                        total.gas_limit += stat.gas_limit.clone();

                        println!("{}: Nonce past: {}, cur: {}, future: {}; FeeCap cur: {}, min-{}: {}, gasLimit: {}", stat.address, stat.past, stat.current, stat.future, stat.below_current, base_fee_lookback, stat.below_past, stat.gas_limit);
                    }

                    println!("-----");
                    println!("total: Nonce past: {}, cur: {}, future: {}; FeeCap cur: {}, min-{}: {}, gasLimit: {}", total.past, total.current, total.future, total.below_current, base_fee_lookback, total.below_past, total.gas_limit);
                }
            }
        }
    }
}

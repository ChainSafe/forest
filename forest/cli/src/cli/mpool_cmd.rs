// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::{HashMap, HashSet};
use clap::Subcommand;
use forest_blocks::tipset_keys_json::TipsetKeysJson;
use forest_json::{address::json::AddressJson, cid::vec::CidJsonVec};
use forest_message::{Message, SignedMessage};
use forest_rpc_client::{chain_ops::*, mpool_ops::*, state_ops::*, wallet_ops::*};
use forest_shim::address::Address;
use fvm_ipld_encoding::Cbor;
use num_bigint::BigInt;

use super::Config;
use crate::cli::handle_rpc_err;

#[derive(Debug, Subcommand)]
pub enum MpoolCommands {
    /// Get pending messages
    Pending {
        /// Print pending messages for addresses in local wallet only
        #[arg(long)]
        local: bool,
        /// Only print `CIDs` of messages in output
        #[arg(long)]
        cids: bool,
        /// Return messages to a given address
        #[arg(long)]
        to: Option<Address>,
        /// Return messages from a given address
        #[arg(long)]
        from: Option<Address>,
    },
    /// Print mempool stats
    Stat {
        /// Number of blocks to look back for minimum `basefee`
        #[arg(short, default_value = "60")]
        base_fee_lookback: u32,
        /// Print stats for addresses in local wallet only
        #[arg(short)]
        local: bool,
    },
}

impl MpoolCommands {
    pub async fn run(&self, config: Config) -> anyhow::Result<()> {
        match *self {
            Self::Pending {
                local,
                cids,
                to,
                from,
            } => {
                let messages = mpool_pending((CidJsonVec(vec![]),), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                let local_addrs = if local {
                    let response = wallet_list((), &config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)?;
                    Some(HashSet::from_iter(response.iter().map(|addr| addr.0)))
                } else {
                    None
                };

                let filtered_messages = messages.iter().filter(|msg| {
                    local_addrs
                        .as_ref()
                        .map(|addrs| addrs.contains(&msg.0.from()))
                        .unwrap_or(true)
                        && to.map(|addr| msg.0.to() == addr).unwrap_or(true)
                        && from.map(|addr| msg.0.from() == addr).unwrap_or(true)
                });
                for msg in filtered_messages {
                    if cids {
                        println!("{}", msg.0.cid().unwrap());
                    } else {
                        println!("{}", serde_json::to_string_pretty(&msg)?);
                    }
                }
                Ok(())
            }
            Self::Stat {
                base_fee_lookback,
                local,
            } => {
                let tipset_json = chain_head(&config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;
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
                    .map_err(handle_rpc_err)?
                    .0;

                    if current_tipset.blocks()[0].parent_base_fee() < &min_base_fee {
                        min_base_fee = current_tipset.blocks()[0].parent_base_fee().clone();
                    }

                    let wallet_response = wallet_list((), &config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)?;

                    let mut addresses = Vec::new();

                    if local {
                        addresses = wallet_response
                            .into_iter()
                            .map(|address| address.0)
                            .collect();
                    }

                    let messages = mpool_pending((CidJsonVec(vec![]),), &config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)?;

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

                    let mut buckets = HashMap::<Address, StatBucket>::default();

                    for message in &messages {
                        if !addresses.iter().any(|&addr| addr == message.0.from()) {
                            continue;
                        }

                        match buckets.get_mut(&message.0.from()) {
                            Some(bucket) => {
                                bucket
                                    .messages
                                    .insert(message.0.sequence(), message.0.to_owned());
                            }
                            None => {
                                buckets.insert(
                                    message.0.from(),
                                    StatBucket {
                                        messages: HashMap::default(),
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
                                    jsonrpc_v2::Error::Full { message, .. } => message,
                                    jsonrpc_v2::Error::Provided { message, .. } => {
                                        message.to_string()
                                    }
                                };

                                println!("{}, err: {}", address, error_message);
                                continue;
                            }
                        };

                        let mut cur = actor_json.0.sequence;

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
                            if message.0.sequence() < actor_json.0.sequence {
                                stat.past += 1;
                            } else if message.0.sequence() > cur {
                                stat.future += 1;
                            } else {
                                stat.current += 1;
                            }

                            if message.0.gas_fee_cap() < current_base_fee {
                                stat.below_current += 1;
                            }

                            if message.0.gas_fee_cap() < min_base_fee {
                                stat.below_past += 1;
                            }

                            stat.gas_limit += message.0.message().gas_limit;
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
                Ok(())
            }
        }
    }
}

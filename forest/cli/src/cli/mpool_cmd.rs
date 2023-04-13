// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::{HashMap, HashSet};
use clap::Subcommand;
use forest_json::cid::vec::CidJsonVec;
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
        #[arg(long, default_value = "60")]
        basefee_lookback: u32,
        /// Print stats for addresses in local wallet only
        #[arg(long)]
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
                basefee_lookback,
                local,
            } => {
                let tipset = chain_head(&config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?
                    .0;

                let curr_base_fee = tipset.blocks()[0].parent_base_fee().to_owned();
                let min_base_fee = {
                    let mut curr_tipset = tipset.clone();
                    let mut min_base_fee = curr_base_fee.clone();
                    // TODO: fix perf issue, this loop is super slow
                    for _ in 0..basefee_lookback {
                        curr_tipset = chain_get_tipset(
                            (curr_tipset.parents().to_owned().into(),),
                            &config.client.rpc_token,
                        )
                        .await
                        .map_err(handle_rpc_err)?
                        .0;

                        min_base_fee =
                            min_base_fee.min(curr_tipset.blocks()[0].parent_base_fee().to_owned());
                    }
                    min_base_fee
                };

                type StatBucket = HashMap<u64, SignedMessage>;

                #[derive(Default)]
                struct MpStat {
                    address: String,
                    past: u64,
                    current: u64,
                    future: u64,
                    below_current: u64,
                    below_past: u64,
                    gas_limit: BigInt,
                }

                let local_addrs = if local {
                    let response = wallet_list((), &config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)?;
                    Some(HashSet::from_iter(response.iter().map(|addr| addr.0)))
                } else {
                    None
                };

                let messages = mpool_pending((CidJsonVec(vec![]),), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                let filtered_messages = messages.iter().filter(|msg| {
                    local_addrs
                        .as_ref()
                        .map(|addrs| addrs.contains(&msg.0.from()))
                        .unwrap_or(true)
                });

                let mut buckets = HashMap::<Address, StatBucket>::default();
                for msg in filtered_messages {
                    buckets
                        .entry(msg.0.from())
                        .or_insert(StatBucket::default())
                        .insert(msg.0.sequence(), msg.0.to_owned());
                }

                let mut stats: Vec<MpStat> = Vec::new();

                for (address, bucket) in buckets {
                    let get_actor_result = state_get_actor(
                        (address.to_owned().into(), tipset.key().to_owned().into()),
                        &config.client.rpc_token,
                    )
                    .await;

                    let actor_state = match get_actor_result {
                        Ok(actor_json) => actor_json.unwrap().0,
                        Err(err) => {
                            let error_message = match err {
                                jsonrpc_v2::Error::Full { message, .. } => message,
                                jsonrpc_v2::Error::Provided { message, .. } => message.to_string(),
                            };

                            println!("{}, err: {}", address, error_message);
                            continue;
                        }
                    };

                    let mut curr_sequence = actor_state.sequence;
                    while bucket.get(&curr_sequence).is_some() {
                        curr_sequence += 1;
                    }

                    let mut stat = MpStat {
                        address: address.to_string(),
                        ..Default::default()
                    };

                    for (_, msg) in bucket {
                        if msg.sequence() < actor_state.sequence {
                            stat.past += 1;
                        } else if msg.sequence() > curr_sequence {
                            stat.future += 1;
                        } else {
                            stat.current += 1;
                        }

                        if msg.gas_fee_cap() < curr_base_fee {
                            stat.below_current += 1;
                        }
                        if msg.gas_fee_cap() < min_base_fee {
                            stat.below_past += 1;
                        }

                        stat.gas_limit += msg.message().gas_limit;
                    }

                    stats.push(stat);
                }

                stats.sort_by(|a, b| a.address.cmp(&b.address));

                let mut total = MpStat::default();

                for stat in stats {
                    total.past += stat.past;
                    total.current += stat.current;
                    total.future += stat.future;
                    total.below_current += stat.below_current;
                    total.below_past += stat.below_past;
                    total.gas_limit += &stat.gas_limit;

                    println!("{}: Nonce past: {}, cur: {}, future: {}; FeeCap cur: {}, min-{}: {}, gasLimit: {}", stat.address, stat.past, stat.current, stat.future, stat.below_current, basefee_lookback, stat.below_past, stat.gas_limit);
                }

                println!("-----");
                println!("total: Nonce past: {}, cur: {}, future: {}; FeeCap cur: {}, min-{}: {}, gasLimit: {}", total.past, total.current, total.future, total.below_current, basefee_lookback, total.below_past, total.gas_limit);

                Ok(())
            }
        }
    }
}

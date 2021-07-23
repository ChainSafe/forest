// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashMap;

use address::Address;
use blocks::tipset_keys_json::TipsetKeysJson;
use jsonrpc_v2::ErrorLike;
use message::SignedMessage;
use structopt::StructOpt;

use cid::json::vec::CidJsonVec;
use rpc_client::chain_ops::*;
use rpc_client::mpool_ops::*;
use rpc_client::wallet_ops::wallet_list;

use crate::cli::handle_rpc_err;

#[derive(Debug, StructOpt)]
pub enum MpoolCommands {
    #[structopt(help = "Get pending messages")]
    Pending,
    #[structopt(help = "Print mempool stats")]
    Stat {
        #[structopt(
            short,
            help = "Number of blocks to lookback for minimum base fee",
            default_value = "60"
        )]
        base_fee_lookback: u32,
        #[structopt(short, help = "Print stats for local addresses only")]
        local: bool,
    },
}

impl MpoolCommands {
    pub async fn run(&self) {
        match self {
            Self::Pending => {
                let res = mpool_pending((CidJsonVec(vec![]),)).await;
                let messages = res.map_err(handle_rpc_err).unwrap();
                println!("{:#?}", messages);
            }
            Self::Stat {
                base_fee_lookback,
                local,
            } => {
                let base_fee_lookback = *base_fee_lookback;
                let local = *local;
                let tipset_json = chain_head().await.map_err(handle_rpc_err).unwrap();
                let tipset = tipset_json.0;

                let current_base_fee = tipset.blocks()[0].parent_base_fee().to_owned();
                let mut min_base_fee = current_base_fee;

                let mut current_tipset = tipset.clone();

                for _ in 1..base_fee_lookback {
                    current_tipset =
                        chain_get_tipset((TipsetKeysJson(current_tipset.parents().to_owned()),))
                            .await
                            .map_err(handle_rpc_err)
                            .unwrap()
                            .0;

                    if current_tipset.blocks()[0].parent_base_fee() < &min_base_fee {
                        min_base_fee = current_tipset.blocks()[0].parent_base_fee().clone();
                    }

                    let wallet_response = wallet_list().await.map_err(handle_rpc_err).unwrap();

                    let mut addresses = Vec::new();

                    if local {
                        addresses = wallet_response
                            .into_iter()
                            .map(|address| address.0)
                            .collect();
                    }

                    let messages = mpool_pending((CidJsonVec(vec![]),))
                        .await
                        .map_err(handle_rpc_err)
                        .unwrap();

                    struct StatBucket {
                        messages: HashMap<Address, SignedMessage>,
                    }

                    let mut buckets = HashMap::<Address, StatBucket>::new();

                    for message in messages {
                        if !addresses.is_empty() {
                            let filter: Vec<&Address> = addresses
                                .iter()
                                .filter(|&addr| addr == &message.message().from)
                                .collect();

                            if filter.is_empty() {
                                continue;
                            }
                        }

                        match buckets.get_mut(&message.message().from) {
                            Some(bucket) => {
                                bucket.messages.insert(message.message().from, message);
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
                }
            }
        }
    }
}

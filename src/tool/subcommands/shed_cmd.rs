// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::BTreeMap;

use crate::rpc_client::ApiInfo;
use anyhow::Context as _;
use clap::{Subcommand, ValueEnum};
use futures::{StreamExt as _, TryFutureExt as _, TryStreamExt as _};
use itertools::Itertools as _;

#[derive(Subcommand)]
pub enum ShedCommands {
    /// Enumerate the tipset CIDs for a span of epochs starting at `height` and working backwards.
    ///
    /// Useful for getting blocks to live test an RPC endpoint.
    SummarizeTipsets {
        /// Multiaddr of the RPC host.
        #[arg(long)]
        host: String,
        /// If omitted, defaults to the HEAD of the node.
        #[arg(long)]
        height: Option<u32>,
        #[arg(long)]
        ancestors: u32,
        #[arg(long, default_value = "yaml")]
        output: GraphAncestorsOutputFormat,
    },
}

#[derive(Clone, ValueEnum)]
pub enum GraphAncestorsOutputFormat {
    Yaml,
    Dot,
}

impl ShedCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            ShedCommands::SummarizeTipsets {
                host,
                height,
                ancestors,
                output,
            } => {
                let client = host
                    .parse::<ApiInfo>()
                    .context("couldn't initialize client")?;
                let head = client.chain_head().await.context("couldn't get HEAD")?;
                let end_height = match height {
                    Some(it) => it,
                    None => head
                        .epoch()
                        .try_into()
                        .context("HEAD epoch out-of-bounds")?,
                };
                let start_height = end_height
                    .checked_sub(ancestors)
                    .context("couldn't set start height")?;

                let epoch2cids = futures::stream::iter((start_height..end_height).map(|epoch| {
                    client
                        .chain_get_tipset_by_height(i64::from(epoch), head.key().clone())
                        .map_ok(|tipset| {
                            let cids = tipset.block_headers().iter().map(|it| it.cid().to_string());
                            (tipset.epoch(), cids.collect::<Vec<_>>())
                        })
                }))
                .buffer_unordered(ancestors.try_into().unwrap_or(usize::MAX))
                .try_collect::<BTreeMap<_, _>>()
                .await?;

                match output {
                    GraphAncestorsOutputFormat::Yaml => {
                        println!("{}", serde_yaml::to_string(&epoch2cids)?);
                    }
                    GraphAncestorsOutputFormat::Dot => {
                        println!("digraph {{");

                        for (epoch, cids) in &epoch2cids {
                            // needs a `cluster` prefix to render as desired
                            println!("\tsubgraph cluster_{} {{", epoch);
                            println!("\t\tlabel = {};", epoch);

                            for cid in cids {
                                println!("\t\t{};", cid);
                            }
                            println!("\t}} // subgraph");
                        }

                        for ((_, cids), (_, next_cids)) in epoch2cids.iter().tuple_windows() {
                            for cid in cids {
                                for next in next_cids {
                                    println!("\t{} -> {} [ style = invis ];", cid, next);
                                }
                            }
                        }

                        println!("}} // digraph");
                    }
                }
            }
        }
        Ok(())
    }
}

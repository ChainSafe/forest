// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{collections::BTreeMap, ffi::OsString};

use super::subcommands::Cli;
use super::subcommands::{GraphAncestorsOutputFormat, Subcommand};
use crate::{cli_shared::logger::setup_minimal_logger, rpc_client::ApiInfo};
use anyhow::Context as _;
use clap::Parser as _;
use futures::{StreamExt as _, TryFutureExt as _, TryStreamExt as _};
use itertools::Itertools as _;

pub fn main<ArgT>(args: impl IntoIterator<Item = ArgT>) -> anyhow::Result<()>
where
    ArgT: Into<OsString> + Clone,
{
    // Capture Cli inputs
    let Cli { cmd } = Cli::parse_from(args);
    setup_minimal_logger();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            // Run command
            match cmd {
                Subcommand::Benchmark(cmd) => cmd.run().await,
                Subcommand::StateMigration(state_migration) => state_migration.run().await,
                Subcommand::Snapshot(cmd) => cmd.run().await,
                Subcommand::Fetch(cmd) => cmd.run().await,
                Subcommand::Archive(cmd) => cmd.run().await,
                Subcommand::DB(cmd) => cmd.run().await,
                Subcommand::Car(cmd) => cmd.run().await,
                Subcommand::Api(cmd) => cmd.run().await,
                Subcommand::SummarizeTipsets {
                    host,
                    ancestors,
                    height,
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

                    let epoch2cids =
                        futures::stream::iter((start_height..end_height).map(|epoch| {
                            client
                                .chain_get_tipset_by_height(i64::from(epoch), head.key().clone())
                                .map_ok(|tipset| {
                                    let cids = tipset
                                        .block_headers()
                                        .iter()
                                        .map(|it| it.cid().to_string());
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

                    Ok(())
                }
            }
        })
}

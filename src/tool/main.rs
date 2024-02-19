// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ffi::OsString;

use super::subcommands::Cli;
use crate::{cli_shared::logger::setup_minimal_logger, rpc_client::ApiInfo};
use anyhow::Context as _;
use clap::Parser as _;

use super::subcommands::Subcommand;

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
                Subcommand::GraphAncestors {
                    host,
                    ancestors,
                    height,
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
                    let tipsets =
                        futures::future::try_join_all((start_height..end_height).map(|epoch| {
                            client.chain_get_tipset_by_height(i64::from(epoch), head.key().clone())
                        }))
                        .await?;

                    println!("digraph {{");
                    for tipset in &tipsets {
                        println!("\tsubgraph \"cluster_{}\"{{", tipset.epoch()); // needs a `cluster` prefix to render as desired
                        println!("\t\tlabel = \"{}\";", tipset.epoch());

                        for block in tipset.block_headers() {
                            println!("\t\t{};", block.cid());
                        }

                        println!("\t}}"); // subgraph
                    }

                    for tipset in tipsets {
                        for block in tipset.block_headers() {
                            for parent in block.parents.cids.clone() {
                                println!("\t{} -> {};", parent, block.cid());
                            }
                        }
                    }

                    println!("}}"); // digraph

                    Ok(())
                }
            }
        })
}

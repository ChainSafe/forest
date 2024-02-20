// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_client::ApiInfo;
use anyhow::Context as _;
use clap::Subcommand;
use futures::{StreamExt as _, TryFutureExt as _, TryStreamExt as _};
use libp2p::Multiaddr;

#[derive(Subcommand)]
pub enum ShedCommands {
    /// Enumerate the tipset CIDs for a span of epochs starting at `height` and working backwards.
    ///
    /// Useful for getting blocks to live test an RPC endpoint.
    SummarizeTipsets {
        /// Multiaddr of the RPC host.
        #[arg(long, default_value = "/ip4/127.0.0.1/tcp/2345/http")]
        host: Multiaddr,
        /// If omitted, defaults to the HEAD of the node.
        #[arg(long)]
        height: Option<u32>,
        #[arg(long)]
        ancestors: u32,
    },
}

impl ShedCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            ShedCommands::SummarizeTipsets {
                host,
                height,
                ancestors,
            } => {
                let client = ApiInfo {
                    multiaddr: host,
                    token: None,
                };
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

                let mut epoch2cids =
                    futures::stream::iter((start_height..=end_height).map(|epoch| {
                        client
                            .chain_get_tipset_by_height(i64::from(epoch), head.key().clone())
                            .map_ok(|tipset| {
                                let cids = tipset.block_headers().iter().map(|it| *it.cid());
                                (tipset.epoch(), cids.collect::<Vec<_>>())
                            })
                    }))
                    .buffered(12);

                while let Some((epoch, cids)) = epoch2cids.try_next().await? {
                    println!("{}:", epoch);
                    for cid in cids {
                        println!("- {}", cid);
                    }
                }
            }
        }
        Ok(())
    }
}

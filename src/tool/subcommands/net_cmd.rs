// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::libp2p::ping::p2p_ping;
use clap::Subcommand;
use libp2p::Multiaddr;
use std::time::Duration;

#[derive(Debug, Subcommand)]
pub enum NetCommands {
    /// Ping a peer via its `multiaddress`
    Ping {
        /// Peer `multiaddress`
        peer: Multiaddr,
        /// The number of times it should ping
        #[arg(short, long, default_value_t = 5)]
        count: usize,
        /// The minimum seconds between pings
        #[arg(short, long, default_value_t = 1)]
        interval: u64,
    },
}

impl NetCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            NetCommands::Ping {
                peer,
                count,
                interval,
            } => {
                println!("PING {peer}");
                let mut n_success = 0;
                let mut total_duration = Duration::default();
                for _ in 0..count {
                    match p2p_ping(peer.clone()).await {
                        Ok(duration) => {
                            n_success += 1;
                            total_duration += duration;
                            println!("Pong received: time={}ms", duration.as_millis())
                        }
                        Err(error) => {
                            println!("Ping failed: error={error}")
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(interval)).await;
                }
                if n_success > 0 {
                    let avg_ms = total_duration.as_millis() / n_success;
                    println!("Average latency: {avg_ms}ms");
                }
            }
        }

        Ok(())
    }
}

// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use crate::{
    libp2p::keypair::get_keypair,
    lotus_json::LotusJson,
    rpc::{
        self,
        chain_api::{ChainGetTipSetByHeight, ChainHead},
        types::ApiTipsetKey,
        RpcMethodExt as _,
    },
    rpc_client::ApiInfo,
};
use anyhow::Context as _;
use base64::{prelude::BASE64_STANDARD, Engine};
use clap::Subcommand;
use futures::{StreamExt as _, TryFutureExt as _, TryStreamExt as _};

#[derive(Subcommand)]
pub enum ShedCommands {
    /// Enumerate the tipset CIDs for a span of epochs starting at `height` and working backwards.
    ///
    /// Useful for getting blocks to live test an RPC endpoint.
    SummarizeTipsets {
        /// If omitted, defaults to the HEAD of the node.
        #[arg(long)]
        height: Option<u32>,
        #[arg(long)]
        ancestors: u32,
    },
    /// Generate a `PeerId` from the given key-pair file.
    PeerIdFromKeyPair {
        /// Path to the key-pair file.
        keypair: PathBuf,
    },
    /// Generate a base64-encoded private key from the given key-pair file.
    /// This effectively transforms Forest's key-pair file into a Lotus-compatible private key.
    PrivateKeyFromKeyPair {
        /// Path to the key-pair file.
        keypair: PathBuf,
    },
    /// Generate a key-pair file from the given base64-encoded private key.
    /// This effectively transforms Lotus's private key into a Forest-compatible key-pair file.
    /// If `output` is not provided, the key-pair is printed to stdout as a base64-encoded string.
    KeyPairFromPrivateKey {
        /// Base64-encoded private key.
        private_key: String,
        /// Path to save the key-pair file.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

impl ShedCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            ShedCommands::SummarizeTipsets { height, ancestors } => {
                let client = rpc::Client::from(ApiInfo::from_env()?);
                let head = ChainHead::call(&client, ()).await?.into_inner();
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
                        ChainGetTipSetByHeight::call(
                            &client,
                            (
                                i64::from(epoch),
                                LotusJson(ApiTipsetKey(Some(head.key().clone()))),
                            ),
                        )
                        .map_ok(|LotusJson(tipset)| {
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
            ShedCommands::PeerIdFromKeyPair { keypair } => {
                let keypair = get_keypair(&keypair)
                    .with_context(|| format!("couldn't get keypair from {}", keypair.display()))?;
                println!("{}", keypair.public().to_peer_id());
            }
            ShedCommands::PrivateKeyFromKeyPair { keypair } => {
                let keypair = get_keypair(&keypair)
                    .with_context(|| format!("couldn't get keypair from {}", keypair.display()))?;
                let encoded = BASE64_STANDARD.encode(keypair.to_protobuf_encoding()?);
                println!("{encoded}");
            }
            ShedCommands::KeyPairFromPrivateKey {
                private_key,
                output,
            } => {
                let private_key = BASE64_STANDARD.decode(private_key)?;
                let keypair_data = libp2p::identity::Keypair::from_protobuf_encoding(&private_key)?
                    // While a keypair can be any type, Forest only supports Ed25519.
                    .try_into_ed25519()?
                    .to_bytes();
                if let Some(output) = output {
                    std::fs::write(output, keypair_data)?;
                } else {
                    println!("{}", BASE64_STANDARD.encode(keypair_data));
                }
            }
        }
        Ok(())
    }
}

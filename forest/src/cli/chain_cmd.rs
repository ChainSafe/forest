// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actors_runtime::runtime::Policy;
use forest_blocks::tipset_keys_json::TipsetKeysJson;
use structopt::StructOpt;

use crate::cli::{cli_error_and_die, handle_rpc_err};

use super::{print_rpc_res, print_rpc_res_cids, print_rpc_res_pretty};
use cid::Cid;
use forest_json::cid::CidJson;
use rpc_client::chain_ops::*;
use time::OffsetDateTime;

#[derive(Debug, StructOpt)]
pub enum ChainCommands {
    /// Retrieves and prints out the block specified by the given CID
    #[structopt(about = "<Cid> Retrieve a block and print its details")]
    Block {
        #[structopt(short, help = "Input a valid CID")]
        cid: String,
    },

    /// Export a snapshot of the chain to <output_path>
    #[structopt(about = "Export chain snapshot to file")]
    Export {
        #[structopt(short, help = "Tipset to start the export from, default is @HEAD")]
        tipset: Option<i64>,
        #[structopt(
            short,
            help = "specify the number of recent state roots to include in the export"
        )]
        recent_stateroots: Option<i64>,
        #[structopt(short, help = "Skips old messages")]
        skip_old_messages: bool,
        #[structopt(short, help = "path of the file to export to")]
        output_path: Option<String>,
    },

    /// Prints out the genesis tipset
    #[structopt(about = "Prints genesis tipset", help = "Prints genesis tipset")]
    Genesis,

    /// Prints out the canonical head of the chain
    #[structopt(about = "Print chain head", help = "Print chain head")]
    Head,

    /// Reads and prints out a message referenced by the specified CID from the
    /// chain blockstore
    #[structopt(about = "<CID> Retrieves and prints messages by CIDs")]
    Message {
        #[structopt(short, help = "Input a valid CID")]
        cid: String,
    },

    /// Reads and prints out ipld nodes referenced by the specified CID from chain
    /// blockstore and returns raw bytes
    #[structopt(about = "<CID> Read the raw bytes of an object")]
    ReadObj {
        #[structopt(short, help = "Input a valid CID")]
        cid: String,
    },
}

impl ChainCommands {
    pub async fn run(&self) {
        match self {
            Self::Block { cid } => {
                let cid: Cid = cid.parse().unwrap();
                print_rpc_res_pretty(chain_get_block((CidJson(cid),)).await);
            }
            Self::Export {
                tipset,
                recent_stateroots,
                skip_old_messages,
                output_path,
            } => {
                let chain_finality = Policy::default().chain_finality;
                let recent_stateroots = match recent_stateroots {
                    Some(recent_stateroots) => {
                        let recent_stateroots = recent_stateroots.to_owned();
                        if recent_stateroots < chain_finality {
                            return cli_error_and_die(
                                &format!(
                                    "\"recent-stateroots\" must be greater than {}",
                                    chain_finality
                                ),
                                1,
                            );
                        }

                        if recent_stateroots == 0 && *skip_old_messages {
                            return cli_error_and_die(
                                "must pass recent-stateroots along with skip-old-messages",
                                1,
                            );
                        }

                        recent_stateroots
                    }
                    None => 0,
                };

                if recent_stateroots == 0 && *skip_old_messages {
                    return cli_error_and_die(
                        "Must pass recent stateroots along with skip-old-messages",
                        1,
                    );
                }

                let chain_head = match chain_head().await {
                    Ok(head) => head.0,
                    Err(_) => return cli_error_and_die("Could not get network head", 1),
                };

                let output_path = match output_path {
                    Some(path) => path.to_owned(),
                    None => {
                        let now = OffsetDateTime::now_utc();
                        format!(
                            "forest_snapshot_{}_{}_{}_height_{}.car",
                            now.year(),
                            now.month(),
                            now.day(),
                            chain_head.epoch()
                        )
                    }
                };

                let epoch = tipset.unwrap_or(chain_head.epoch());

                let params = (
                    epoch,
                    recent_stateroots,
                    *skip_old_messages,
                    output_path.clone(),
                    TipsetKeysJson(chain_head.key().clone()),
                );

                let out = chain_export(params)
                    .await
                    .map_err(handle_rpc_err)
                    .expect("errors ara handled by handle_rpc_error. This is safe");

                println!("Export completed. Snapshot located at {}", out.display());
                println!("\nYou can check the snapshot's integrity using the provided checksum in the same directory by issuing the following command.\n\n`sha256sum --check {}.sha256sum`", out.display());
            }
            Self::Genesis => {
                print_rpc_res_pretty(chain_get_genesis().await);
            }
            Self::Head => {
                print_rpc_res_cids(chain_head().await);
            }
            Self::Message { cid } => {
                let cid: Cid = cid.parse().unwrap();
                print_rpc_res_pretty(chain_get_message((CidJson(cid),)).await);
            }
            Self::ReadObj { cid } => {
                let cid: Cid = cid.parse().unwrap();
                print_rpc_res(chain_read_obj((CidJson(cid),)).await);
            }
        }
    }
}

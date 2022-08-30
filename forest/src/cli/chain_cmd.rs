// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_blocks::tipset_keys_json::TipsetKeysJson;
use structopt::StructOpt;

use super::{print_rpc_res, print_rpc_res_cids, print_rpc_res_pretty};
use crate::cli::{cli_error_and_die, handle_rpc_err};
use cid::Cid;
use forest_json::cid::CidJson;
use forest_rpc_client::chain_ops::*;
use std::collections::HashMap;
use strfmt::strfmt;
use time::OffsetDateTime;

const OUTPUT_PATH_DEFAULT_FORMAT: &str =
    "forest_snapshot_{chain}_{year}-{month}-{day}_height_{height}.car";

#[derive(Debug, StructOpt)]
pub enum ChainCommands {
    /// Retrieves and prints out the block specified by the given CID
    #[structopt(about = "<Cid> Retrieve a block and print its details")]
    Block {
        #[structopt(short, help = "Input a valid CID")]
        cid: String,
    },

    /// Export a snapshot of the chain to `<output_path>`
    #[structopt(about = "Export chain snapshot to file")]
    Export {
        /// Tipset to start the export from, default is the chain head
        #[structopt(short, long)]
        tipset: Option<i64>,
        /// Specify the number of recent state roots to include in the export.
        #[structopt(short, long, default_value = "2000")]
        recent_stateroots: i64,
        /// Include old messages
        #[structopt(short, long)]
        include_old_messages: bool,
        /// Snapshot output path. Default to `forest_snapshot_{chain}_{year}-{month}-{day}_height_{height}.car`
        /// Date is in ISO 8601 date format.
        /// Arguments:
        ///  - chain - chain name e.g. `mainnet`
        ///  - year
        ///  - month
        ///  - day
        ///  - height - the epoch
        #[structopt(short, default_value = OUTPUT_PATH_DEFAULT_FORMAT, verbatim_doc_comment)]
        output_path: String,
    },

    /// Prints out the genesis tipset
    #[structopt(about = "Prints genesis tipset", help = "Prints genesis tipset")]
    Genesis,

    /// Prints out the canonical head of the chain
    #[structopt(about = "Print chain head", help = "Print chain head")]
    Head,

    /// Reads and prints out a message referenced by the specified CID from the
    /// chain block store
    #[structopt(about = "<CID> Retrieves and prints messages by CIDs")]
    Message {
        #[structopt(short, help = "Input a valid CID")]
        cid: String,
    },

    /// Reads and prints out IPLD nodes referenced by the specified CID from chain
    /// block store and returns raw bytes
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
                output_path,
                include_old_messages,
            } => {
                let chain_head = match chain_head().await {
                    Ok(head) => head.0,
                    Err(_) => cli_error_and_die("Could not get network head", 1),
                };

                let epoch = tipset.unwrap_or(chain_head.epoch());

                let now = OffsetDateTime::now_utc();

                let month_string = format!("{:02}", now.month() as u8);
                let year = now.year();
                let day_string = format!("{:02}", now.day() as u8);
                let chain_name = chain_get_name().await.map_err(handle_rpc_err).unwrap();

                let vars = HashMap::from([
                    ("year".to_string(), year.to_string()),
                    ("month".to_string(), month_string),
                    ("day".to_string(), day_string),
                    ("chain".to_string(), chain_name),
                    ("height".to_string(), epoch.to_string()),
                ]);
                let output_path = match strfmt(output_path, &vars) {
                    Ok(path) => path,
                    Err(e) => {
                        cli_error_and_die(format!("Unparsable string error: {}", e), 1);
                    }
                };

                let params = (
                    epoch,
                    *recent_stateroots,
                    *include_old_messages,
                    output_path,
                    TipsetKeysJson(chain_head.key().clone()),
                );

                // infallible unwrap
                let out = chain_export(params).await.map_err(handle_rpc_err).unwrap();

                println!("Export completed. Snapshot located at {}", out.display());
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

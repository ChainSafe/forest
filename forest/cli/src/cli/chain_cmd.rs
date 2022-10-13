// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use log::warn;
use structopt::StructOpt;

use super::*;
use cid::Cid;
use forest_json::cid::CidJson;
use forest_rpc_client::chain_ops::*;
use std::path::PathBuf;

#[derive(Debug, StructOpt)]
pub enum ChainCommands {
    /// Retrieves and prints out the block specified by the given CID
    Block {
        /// Input a valid CID
        #[structopt(short)]
        cid: String,
    },

    /// (Deprecated) Export a snapshot of the chain to `<output_path>`
    /// Use `forest-cli snapshot export` instead.
    // TODO: Remove this
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
        output_path: PathBuf,
        /// Skip creating the checksum file.
        #[structopt(long)]
        skip_checksum: bool,
    },

    /// Prints out the genesis tipset
    Genesis,

    /// Prints out the canonical head of the chain
    Head,

    /// Reads and prints out a message referenced by the specified CID from the
    /// chain block store
    Message {
        /// Input a valid CID
        #[structopt(short)]
        cid: String,
    },

    /// Reads and prints out IPLD nodes referenced by the specified CID from chain
    /// block store and returns raw bytes
    ReadObj {
        /// Input a valid CID
        #[structopt(short)]
        cid: String,
    },

    /// (Deprecated) Fetches the most recent snapshot from a trusted, pre-defined location.
    /// Use `forest-cli snapshot fetch` instead.
    // TODO: Remove this
    Fetch {
        /// Directory to which the snapshot should be downloaded. If not provided, it will be saved
        /// in default Forest data location.
        #[structopt(short, long)]
        snapshot_dir: Option<PathBuf>,
    },
}

impl ChainCommands {
    pub async fn run(&self, config: Config) {
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
                skip_checksum,
            } => {
                warn!("Deprecated, use `forest-cli snapshot export` instead.");
                let cmd = SnapshotCommands::Export {
                    tipset: *tipset,
                    recent_stateroots: *recent_stateroots,
                    output_path: output_path.clone(),
                    include_old_messages: *include_old_messages,
                    skip_checksum: *skip_checksum,
                };
                cmd.run(config).await
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
            Self::Fetch { snapshot_dir } => {
                warn!("Deprecated, use `forest-cli snapshot fetch` instead.");
                let cmd = SnapshotCommands::Fetch {
                    snapshot_dir: snapshot_dir.clone(),
                };
                cmd.run(config).await
            }
        }
    }
}

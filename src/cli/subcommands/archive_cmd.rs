// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Archives are key-value pairs encoded as
//! [CAR files](https://ipld.io/specs/transport/car/carv1/). The key-value pairs
//! represent a directed, acyclic graph (DAG). This graph is often a subset of a larger
//! graph and references to missing keys are common.
//!
//! Each graph contains blocks, messages, state trees, and miscellaneous data
//! such as compiled `WASM` code. The amount of data differs greatly in different
//! kinds of archives. While there are no fixed definitions, there are three
//! common kind of archives:
//! - A full archive contains a complete graph with no missing nodes. These
//!   archives are large (14 TiB for Filecoin's mainnet) and only used in special
//!   situations.
//! - A lite-archive typically has roughly 3 million blocks, 2000 complete sets of
//!   state-roots, and 2000 sets of messages. These archives usually take up
//!   roughly 100 GiB.
//! - A diff-archive contains the subset of nodes that are _not_ shared by two
//!   other archives. These archives are much smaller but can rarely be used on
//!   their own. They are typically merged with other archives before use.
//!
//! The sub-commands in this module manipulate archive files without needing a
//! running Forest-daemon or a separate database. Operations are carried out
//! directly on CAR files.
//!
//! Additional reading: [`crate::db::car::plain`]

use crate::chain::ChainEpochDelta;
use crate::shim::clock::ChainEpoch;
use crate::utils::bail_moved_cmd;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum ArchiveCommands {
    // This subcommand is hidden and only here to help users migrating to forest-tool
    #[command(hide = true)]
    Info { snapshot: PathBuf },
    // This subcommand is hidden and only here to help users migrating to forest-tool
    #[command(hide = true)]
    Export {
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
        #[arg(short, long, default_value = ".", verbatim_doc_comment)]
        output_path: PathBuf,
        #[arg(short, long)]
        epoch: Option<ChainEpoch>,
        #[arg(short, long, default_value_t = 2000)]
        depth: ChainEpochDelta,
        #[arg(short, long)]
        diff: Option<ChainEpoch>,
        #[arg(short, long)]
        diff_depth: Option<ChainEpochDelta>,
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    // This subcommand is hidden and only here to help users migrating to forest-tool
    #[command(hide = true)]
    Checkpoints {
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
    },
}

impl ArchiveCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Info { .. } => bail_moved_cmd("archive info", "forest-tool archive info"),
            Self::Export { .. } => bail_moved_cmd("archive export", "forest-tool archive export"),
            Self::Checkpoints { .. } => {
                bail_moved_cmd("archive checkpoints", "forest-tool archive checkpoints")
            }
        }
    }
}

// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Archives are key-value pairs encoded as
//! [CAR files](https://ipld.io/specs/transport/car/carv1/). The key-value pairs
//! represent a directed, acyclic graph (DAG). This graph is often a subset of a larger
//! graph and references to missing keys are common.
//!
//! Each graph contains blocks, messages, state trees, and miscellaneous data
//! such as compiled WASM code. The amount of data differs greatly in different
//! kinds of archives. While there are no fixed definitions, there are three
//! common kind of archives:
//!   A full archive contains a complete graph with no missing nodes. These
//!   archives are large (14TiB for Filecoin's mainnet) and only used in special
//!   situations.
//!   A lite-archive typically has ~3 million blocks, 2000 complete sets of
//!   state-roots, and 2000 sets of messages. These archives usually take up
//!   ~100GiB.
//!   A diff-archive contains the subset of nodes that are _not_ shared by two
//!   other archives. These archives are much smaller but can rarely be used on
//!   their own. They are typically merged with other archives before use.
//!
//! Additional reading:
//!     <https://github.com/ChainSafe/forest/blob/main/documentation/src/developer_documentation/filecoin_archive.md>
//!     [`CarBackedBlockstore`]

use super::Config;
use crate::blocks::{Tipset, TipsetKeys};
use crate::car_backed_blockstore::CarBackedBlockstore;
use crate::networks::{calibnet, mainnet};
use crate::shim::clock::ChainEpoch;
use anyhow::{bail, Context as _};
use clap::Subcommand;
use fvm_ipld_blockstore::Blockstore;
use indicatif::ProgressIterator;
use itertools::Itertools;
use std::io::{Read, Seek};
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum ArchiveCommands {
    /// Show basic information about an archive.
    Info {
        /// Path to an uncompressed archive (CAR)
        snapshot: PathBuf,
    },
}

impl ArchiveCommands {
    pub async fn run(self, config: Config) -> anyhow::Result<()> {
        match self {
            Self::Info { snapshot } => {
                let info = ArchiveInfo::from_file(snapshot)?;
                println!("{:?}", info);
                Ok(())
            }
        }
    }
}

#[derive(Debug)]
struct ArchiveInfo {
    variant: String,
    network: String,
    epoch: ChainEpoch,
    tipsets: ChainEpoch,
    messages: ChainEpoch,
}

impl ArchiveInfo {
    fn from_file(path: PathBuf) -> anyhow::Result<Self> {
        Self::from_reader(std::fs::File::open(path)?)
    }

    fn from_reader(reader: impl Read + Seek) -> anyhow::Result<Self> {
        let store = CarBackedBlockstore::new(reader)
            .context("couldn't read input CAR file - is it compressed?")?;
        Ok(ArchiveInfo {
            variant: "CARv1".into(),
            network: "Unknown".into(),
            epoch: 0,
            tipsets: 0,
            messages: 0,
        })
    }
}

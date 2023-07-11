// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::car_backed_blockstore::CarBackedBlockstore;
use crate::shim::clock::ChainEpoch;
use anyhow::{bail, Context as _};
use clap::Subcommand;
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

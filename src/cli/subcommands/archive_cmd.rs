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
    pub async fn run(self, _config: Config) -> anyhow::Result<()> {
        match self {
            Self::Info { snapshot } => {
                println!("{}", ArchiveInfo::from_file(snapshot)?);
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

impl std::fmt::Display for ArchiveInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "CAR format:    {}", self.variant)?;
        writeln!(f, "Network:       {}", self.network)?;
        writeln!(f, "Epoch:         {}", self.epoch)?;
        writeln!(f, "State-roots:   {}", self.epoch - self.tipsets + 1)?;
        write!(f, "Messages sets: {}", self.epoch - self.messages + 1)?;
        Ok(())
    }
}

impl ArchiveInfo {
    // Scan a CAR file to identify which network it belongs to and how many
    // tipsets/messages are available. Progress is rendered to stdout.
    fn from_file(path: PathBuf) -> anyhow::Result<Self> {
        Self::from_reader(std::fs::File::open(path)?)
    }

    // Scan a CAR archive to identify which network it belongs to and how many
    // tipsets/messages are available. Progress is rendered to stdout.
    fn from_reader(reader: impl Read + Seek) -> anyhow::Result<Self> {
        Self::from_reader_with(reader, true)
    }

    // Scan a CAR archive to identify which network it belongs to and how many
    // tipsets/messages are available. Progress is optionally rendered to
    // stdout.
    fn from_reader_with(reader: impl Read + Seek, progress: bool) -> anyhow::Result<Self> {
        let store = CarBackedBlockstore::new(reader)
            .context("couldn't read input CAR file - is it compressed?")?;

        let root = Tipset::load(&store, &TipsetKeys::new(store.roots()))?
            .context("Missing root tipset")?;
        let root_epoch = root.epoch();

        let tipsets = itertools::unfold(Some(root.clone()), |tipset| {
            let child = tipset.take()?;
            *tipset = Tipset::load(&store, child.parents()).ok().flatten();
            Some(child)
        });

        let windowed = (std::iter::once(root).chain(tipsets)).tuple_windows();

        let mut network: String = "unknown".into();
        let mut lowest_stateroot_epoch = root_epoch;
        let mut lowest_message_epoch = root_epoch;

        let iter = if progress {
            itertools::Either::Left(windowed.progress_count(root_epoch as u64))
        } else {
            itertools::Either::Right(windowed)
        };

        for (parent, tipset) in iter {
            if tipset.epoch() >= parent.epoch() && parent.epoch() != root_epoch {
                bail!("Broken invariant: non-sequential epochs");
            }

            if tipset.epoch() < 0 {
                bail!("Broken invariant: tipset with negative epoch");
            }

            // Update the lowest-stateroot-epoch only if our parent also has a
            // state-root. The genesis state-root is usually available but we're
            // not interested in that.
            if lowest_stateroot_epoch == parent.epoch() && store.has(tipset.parent_state())? {
                lowest_stateroot_epoch = tipset.epoch();
            }
            if lowest_message_epoch == parent.epoch()
                && store.has(tipset.min_ticket_block().messages())?
            {
                lowest_message_epoch = tipset.epoch();
            }

            if tipset.epoch() == 0 {
                if tipset.min_ticket_block().cid().to_string() == calibnet::GENESIS_CID {
                    network = "calibnet".into();
                } else if tipset.min_ticket_block().cid().to_string() == mainnet::GENESIS_CID {
                    network = "mainnet".into();
                }
            }

            // If we've already found the lowest-stateroot-epoch and
            // lowest-message-epoch then we can skip scanning the rest of the
            // archive when we find a checkpoint.
            let may_skip =
                lowest_stateroot_epoch != tipset.epoch() && lowest_message_epoch != tipset.epoch();
            if may_skip {
                if let Some(genesis_keys) =
                    crate::chain::store::index::checkpoint_tipsets::genesis_from_checkpoint_tipset(
                        tipset.key(),
                    )
                {
                    let genesis_cid = genesis_keys.cids[0];
                    if genesis_cid.to_string() == calibnet::GENESIS_CID {
                        network = "calibnet".into();
                    } else if genesis_cid.to_string() == mainnet::GENESIS_CID {
                        network = "mainnet".into();
                    }
                    break;
                }
            }
        }

        Ok(ArchiveInfo {
            variant: "CARv1".into(),
            network,
            epoch: root_epoch,
            tipsets: lowest_stateroot_epoch,
            messages: lowest_message_epoch,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_info_calibnet() {
        let info =
            ArchiveInfo::from_reader_with(std::io::Cursor::new(calibnet::DEFAULT_GENESIS), false)
                .unwrap();
        assert!(info.network == "calibnet");
        assert!(info.epoch == 0);
    }

    #[test]
    fn archive_info_mainnet() {
        let info =
            ArchiveInfo::from_reader_with(std::io::Cursor::new(mainnet::DEFAULT_GENESIS), false)
                .unwrap();
        assert!(info.network == "mainnet");
        assert!(info.epoch == 0);
    }
}

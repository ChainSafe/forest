// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::db::car::{AnyCar, RandomAccessFileReader};
use crate::networks::{calibnet, mainnet};
use crate::shim::clock::ChainEpoch;
use anyhow::bail;
use clap::Subcommand;
use fvm_ipld_blockstore::Blockstore;
use indicatif::ProgressIterator;
use itertools::Itertools;
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
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Info { snapshot } => {
                println!("{}", ArchiveInfo::from_store(AnyCar::try_from(snapshot)?)?);
                Ok(())
            }
        }
    }
}

#[derive(Debug)]
pub struct ArchiveInfo {
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
    // Scan a CAR archive to identify which network it belongs to and how many
    // tipsets/messages are available. Progress is rendered to stdout.
    fn from_store(store: AnyCar<impl RandomAccessFileReader>) -> anyhow::Result<Self> {
        Self::from_store_with(store, true)
    }

    // Scan a CAR archive to identify which network it belongs to and how many
    // tipsets/messages are available. Progress is optionally rendered to
    // stdout.
    fn from_store_with(
        store: AnyCar<impl RandomAccessFileReader>,
        progress: bool,
    ) -> anyhow::Result<Self> {
        let root = store.heaviest_tipset()?;
        let root_epoch = root.epoch();

        let tipsets = root.clone().chain(&store);

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
                if tipset.min_ticket_block().cid() == &*calibnet::GENESIS_CID {
                    network = "calibnet".into();
                } else if tipset.min_ticket_block().cid() == &*mainnet::GENESIS_CID {
                    network = "mainnet".into();
                }
            }

            // If we've already found the lowest-stateroot-epoch and
            // lowest-message-epoch then we can skip scanning the rest of the
            // archive when we find a checkpoint.
            let may_skip =
                lowest_stateroot_epoch != tipset.epoch() && lowest_message_epoch != tipset.epoch();
            if may_skip {
                let genesis_block = tipset.genesis(&store)?;
                if genesis_block.cid() == &*calibnet::GENESIS_CID {
                    network = "calibnet".into();
                } else if genesis_block.cid() == &*mainnet::GENESIS_CID {
                    network = "mainnet".into();
                }
                break;
            }
        }

        Ok(ArchiveInfo {
            variant: store.variant().to_string(),
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
        let info = ArchiveInfo::from_store_with(
            AnyCar::try_from(calibnet::DEFAULT_GENESIS).unwrap(),
            false,
        )
        .unwrap();
        assert_eq!(info.network, "calibnet");
        assert_eq!(info.epoch, 0);
    }

    #[test]
    fn archive_info_mainnet() {
        let info = ArchiveInfo::from_store_with(
            AnyCar::try_from(mainnet::DEFAULT_GENESIS).unwrap(),
            false,
        )
        .unwrap();
        assert_eq!(info.network, "mainnet");
        assert_eq!(info.epoch, 0);
    }
}

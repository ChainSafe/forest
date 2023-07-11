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
//! The subcommands in this module manipulate archive files without needing a
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
        writeln!(f, "CAR format:         {}", self.variant)?;
        writeln!(f, "Network:            {}", self.network)?;
        writeln!(f, "Epoch:              {}", self.epoch)?;
        writeln!(f, "  With state-roots: {}", self.epoch - self.tipsets + 1)?;
        write!(f, "  With messages:    {}", self.epoch - self.messages + 1)?;
        Ok(())
    }
}

impl ArchiveInfo {
    fn from_file(path: PathBuf) -> anyhow::Result<Self> {
        Self::from_reader(std::fs::File::open(path)?)
    }

    fn from_reader(reader: impl Read + Seek) -> anyhow::Result<Self> {
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

        for (parent, tipset) in windowed.progress_count(root_epoch as u64) {
            if tipset.epoch() >= parent.epoch() && parent.epoch() != root_epoch {
                bail!("Broken invariant: non-sequential epochs");
            }

            if tipset.epoch() < 0 {
                bail!("Broken invariant: tipset with negative epoch");
            }

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

            let may_skip = lowest_stateroot_epoch != tipset.epoch()
                && lowest_stateroot_epoch != tipset.epoch();
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

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
//! Additional reading: [`crate::car_backed_blockstore`]

use crate::blocks::{Tipset, TipsetKeys};
use crate::car_backed_blockstore::UncompressedCarV1BackedBlockstore;
use crate::chain::index::ResolveNullTipset;
use crate::chain::{ChainEpochDelta, ChainStore};
use crate::cli_shared::{snapshot, snapshot::TrustedVendor};
use crate::genesis::read_genesis_header;
use crate::networks::{calibnet, mainnet, NetworkChain};
use crate::shim::clock::{ChainEpoch, EPOCHS_IN_DAY};
use crate::Config;
use anyhow::{bail, Context as _};
use chrono::Utc;
use clap::Subcommand;
use fvm_ipld_blockstore::Blockstore;
use indicatif::ProgressIterator;
use itertools::Itertools;
use sha2::Sha256;
use std::io::{Read, Seek};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio_util::compat::TokioAsyncReadCompatExt;
use tracing::info;

#[derive(Debug, Subcommand)]
pub enum ArchiveCommands {
    /// Show basic information about an archive.
    Info {
        /// Path to an uncompressed archive (CAR)
        snapshot: PathBuf,
    },
    /// Trim a snapshot of the chain and write it to `<output_path>`
    Export {
        /// Snapshot input path. Currently supports only `.car` file format.
        #[arg(index = 1)]
        input_path: PathBuf,
        /// Snapshot output filename or directory. Defaults to
        /// `./forest_snapshot_{chain}_{year}-{month}-{day}_height_{epoch}.car.zst`.
        #[arg(short, default_value = ".", verbatim_doc_comment)]
        output_path: PathBuf,
        /// Latest epoch that has to be exported for this snapshot, the upper bound. This value
        /// cannot be greater than the latest epoch available in the input snapshot.
        #[arg(short)]
        epoch: ChainEpoch,
        /// How far back we want to go. Think of it as `$epoch - $depth`, the lower bound of this
        /// snapshot. This value cannot be less than `chain finality`, which is currently assumed
        /// to be `900`. If this ever changes - the actual value is specified in the error message
        /// that is thrown in case `depth` value is too low.
        /// This parameter is optional due to the fact that we need to fetch the exact default
        /// dynamically from configuration.
        // Potentially replace with dynamic default: https://github.com/ChainSafe/forest/issues/3182
        #[arg(short)]
        depth: Option<ChainEpochDelta>,
    },
    /// Print block headers at 30 day interval for a snapshot file
    Checkpoints {
        /// Path to snapshot file.
        snapshot: PathBuf,
    },
}

impl ArchiveCommands {
    pub async fn run(self, config: Config) -> anyhow::Result<()> {
        match self {
            Self::Info { snapshot } => {
                println!("{}", ArchiveInfo::from_file(snapshot)?);
                Ok(())
            }
            Self::Export {
                input_path,
                output_path,
                epoch,
                depth,
            } => {
                let chain_finality = config.chain.policy.chain_finality;
                let depth = depth.unwrap_or(chain_finality);
                if depth < chain_finality {
                    bail!("depth has to be at least {}", chain_finality);
                }

                let reader = std::fs::File::open(&input_path)?;

                info!(
                    "indexing a car-backed store using snapshot: {}",
                    input_path.to_str().unwrap_or_default()
                );

                do_export(config, reader, output_path, epoch, depth).await
            }
            Self::Checkpoints { snapshot } => print_checkpoints(snapshot),
        }
    }
}

// This does nothing if the output path is a file. If it is a directory - it produces the following:
// `./forest_snapshot_{chain}_{year}-{month}-{day}_height_{epoch}.car.zst`.
fn build_output_path(chain: String, epoch: ChainEpoch, output_path: PathBuf) -> PathBuf {
    match output_path.is_dir() {
        true => output_path.join(snapshot::filename(
            TrustedVendor::Forest,
            chain,
            Utc::now().date_naive(),
            epoch,
        )),
        false => output_path.clone(),
    }
}

async fn do_export(
    config: Config,
    reader: impl Read + Seek + Send + Sync,
    output_path: PathBuf,
    epoch: ChainEpoch,
    depth: ChainEpochDelta,
) -> anyhow::Result<()> {
    let store = Arc::new(
        UncompressedCarV1BackedBlockstore::new(reader)
            .context("couldn't read input CAR file - it's either compressed or corrupt")?,
    );

    let genesis = read_genesis_header(
        config.client.genesis_file.as_ref(),
        config.chain.genesis_bytes(),
        &store,
    )
    .await?;

    let tmp_chain_dir = TempDir::new()?;

    let chain_store = Arc::new(ChainStore::new(
        store,
        config.chain.clone(),
        &genesis,
        tmp_chain_dir.path(),
    )?);

    let ts = chain_store.tipset_from_keys(&TipsetKeys::new(chain_store.db.roots()))?;

    info!("looking up a tipset by epoch: {}", epoch);

    let ts = chain_store
        .chain_index
        .tipset_by_height(epoch, ts, ResolveNullTipset::TakeOlder)
        .context("unable to get a tipset at given height")?;

    let output_path = build_output_path(config.chain.network.to_string(), epoch, output_path);

    let writer = tokio::fs::File::create(&output_path)
        .await
        .context(format!(
            "unable to create a snapshot - is the output path '{}' correct?",
            output_path.to_str().unwrap_or_default()
        ))?;

    info!(
        "exporting snapshot at location: {}",
        output_path.to_str().unwrap_or_default()
    );

    chain_store
        .export::<_, Sha256>(&ts, depth, writer.compat(), true, true)
        .await?;

    Ok(())
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
        let store = UncompressedCarV1BackedBlockstore::new(reader)
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
                match tipset.genesis(&store).ok() {
                    Some(genesis_block) => {
                        if genesis_block.cid() == &*calibnet::GENESIS_CID {
                            network = "calibnet".into();
                        } else if genesis_block.cid() == &*mainnet::GENESIS_CID {
                            network = "mainnet".into();
                        }
                    }
                    None => {
                        break;
                    }
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

// Print a mapping of epochs to block headers in yaml format. This mapping can
// be used by Forest to quickly identify tipsets.
fn print_checkpoints(snapshot: PathBuf) -> anyhow::Result<()> {
    let file = std::fs::File::open(snapshot)?;
    let store = UncompressedCarV1BackedBlockstore::new(file)
        .context("couldn't read input CAR file - is it compressed?")?;
    let root = Tipset::load_required(&store, &TipsetKeys::new(store.roots()))?;

    let genesis = root.genesis(&store)?;
    let chain_name = if genesis.cid() == &*calibnet::GENESIS_CID {
        NetworkChain::Calibnet
    } else if genesis.cid() == &*mainnet::GENESIS_CID {
        NetworkChain::Mainnet
    } else {
        bail!("Unrecognizable genesis block");
    };

    println!("{}:", chain_name);
    for (epoch, cid) in list_checkpoints(store, root) {
        println!("  {}: {}", epoch, cid);
    }
    Ok(())
}

fn list_checkpoints(
    db: impl Blockstore,
    root: Tipset,
) -> impl Iterator<Item = (ChainEpoch, cid::Cid)> {
    let interval = EPOCHS_IN_DAY * 30;
    let mut target_epoch = root.epoch() - root.epoch() % interval;
    root.chain(db).filter_map(move |tipset| {
        if tipset.epoch() <= target_epoch && tipset.epoch() != 0 {
            target_epoch -= interval;
            Some((tipset.epoch(), *tipset.min_ticket_block().cid()))
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_compression::tokio::bufread::ZstdDecoder;
    use fvm_ipld_car::CarReader;
    use tokio::io::BufReader;

    #[test]
    fn archive_info_calibnet() {
        let info =
            ArchiveInfo::from_reader_with(std::io::Cursor::new(calibnet::DEFAULT_GENESIS), false)
                .unwrap();
        assert_eq!(info.network, "calibnet");
        assert_eq!(info.epoch, 0);
    }

    #[test]
    fn archive_info_mainnet() {
        let info =
            ArchiveInfo::from_reader_with(std::io::Cursor::new(mainnet::DEFAULT_GENESIS), false)
                .unwrap();
        assert_eq!(info.network, "mainnet");
        assert_eq!(info.epoch, 0);
    }

    #[tokio::test]
    async fn export() {
        let config = Config::default();
        let output_path = TempDir::new().unwrap();
        do_export(
            config.clone(),
            std::io::Cursor::new(calibnet::DEFAULT_GENESIS),
            output_path.path().into(),
            0,
            1,
        )
        .await
        .unwrap();
        let file = tokio::fs::File::open(build_output_path(
            config.chain.network.to_string(),
            0,
            output_path.path().into(),
        ))
        .await
        .unwrap();
        let file = BufReader::new(file);
        CarReader::new(ZstdDecoder::new(file).compat())
            .await
            .unwrap();
    }
}

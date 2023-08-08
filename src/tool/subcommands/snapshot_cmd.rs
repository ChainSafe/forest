// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::Tipset;
use crate::chain::index::ChainIndex;
use crate::cli_shared::snapshot;
use crate::daemon::bundle::load_actor_bundles;
use crate::db::car::ManyCar;
use crate::fil_cns::composition as cns;
use crate::ipld::{recurse_links_hash, CidHashSet};
use crate::networks::{calibnet, mainnet, ChainConfig, NetworkChain};
use crate::shim::machine::MultiEngine;
use crate::utils::proofs_api::paramfetch::ensure_params_downloaded;
use anyhow::{bail, Result};
use cid::Cid;
use clap::Subcommand;
use fvm_ipld_blockstore::Blockstore;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Subcommand)]
pub enum SnapshotCommands {
    /// Fetches the most recent snapshot from a trusted, pre-defined location.
    Fetch {
        #[arg(short, long, default_value = ".")]
        directory: PathBuf,
        /// Vendor to fetch the snapshot from
        #[arg(short, long, value_enum, default_value_t = snapshot::TrustedVendor::default())]
        vendor: snapshot::TrustedVendor,
    },

    /// Validates the snapshot.
    Validate {
        /// Number of recent epochs to scan for broken links
        #[arg(long, default_value_t = 2000)]
        check_links: u32,
        /// Assert the snapshot belongs to this network. If left blank, the
        /// network will be inferred before executing messages.
        #[arg(long)]
        check_network: Option<crate::networks::NetworkChain>,
        /// Number of recent epochs to scan for bad messages/transactions
        #[arg(long, default_value_t = 60)]
        check_stateroots: u32,
        /// Path to a snapshot CAR, which may be zstd compressed
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
    },
}

impl SnapshotCommands {
    pub async fn run(self) -> Result<()> {
        match self {
            Self::Fetch { directory, vendor } => {
                let config = read_config()?;
                match snapshot::fetch(&directory, &config.chain.network, vendor).await {
                    Ok(out) => {
                        println!("{}", out.display());
                        Ok(())
                    }
                    Err(e) => cli_error_and_die(format!("Failed fetching the snapshot: {e}"), 1),
                }
            }
            Self::Validate {
                check_links,
                check_network,
                check_stateroots,
                snapshot_files,
            } => {
                let store = ManyCar::try_from(snapshot_files)?;
                validate_with_blockstore(
                    store.heaviest_tipset()?,
                    Arc::new(store),
                    check_links,
                    check_network,
                    check_stateroots,
                )
                .await
            }
        }
    }
}

// Check the validity of a snapshot by looking at IPLD links, the genesis block,
// and message output. More checks may be added in the future.
//
// If the snapshot is valid, the output should look like this:
//     Checking IPLD integrity:       ✅ verified!
//     Identifying genesis block:     ✅ found!
//     Verifying network identity:    ✅ verified!
//     Running tipset transactions:   ✅ verified!
//   Snapshot is valid
//
// If we receive a mainnet snapshot but expect a calibnet snapshot, the output
// should look like this:
//     Checking IPLD integrity:       ✅ verified!
//     Identifying genesis block:     ✅ found!
//     Verifying network identity:    ❌ wrong!
//   Error: Expected mainnet but found calibnet
async fn validate_with_blockstore<BlockstoreT>(
    root: Tipset,
    store: Arc<BlockstoreT>,
    check_links: u32,
    check_network: Option<NetworkChain>,
    check_stateroots: u32,
) -> Result<()>
where
    BlockstoreT: Blockstore + Send + Sync + 'static,
{
    if check_links != 0 {
        validate_ipld_links(root.clone(), &store, check_links).await?;
    }

    if let Some(expected_network) = &check_network {
        let actual_network = query_network(&root, &store)?;
        // Somewhat silly use of a spinner but this makes the checks line up nicely.
        let pb = validation_spinner("Verifying network identity:");
        if expected_network != &actual_network {
            pb.finish_with_message("❌ wrong!");
            bail!("Expected {} but found {}", expected_network, actual_network);
        } else {
            pb.finish_with_message("✅ verified!");
        }
    }

    if check_stateroots != 0 {
        let network = check_network
            .map(anyhow::Ok)
            .unwrap_or_else(|| query_network(&root, &store))?;
        validate_stateroots(root, &store, network, check_stateroots).await?;
    }

    println!("Snapshot is valid");
    Ok(())
}

// The Filecoin block chain is a DAG of Ipld nodes. The complete graph isn't
// required to sync to the network and snapshot files usually disgard data after
// 2000 epochs. Validity can be verified by ensuring there are no bad IPLD or
// broken links in the N most recent epochs.
async fn validate_ipld_links<DB>(ts: Tipset, db: &DB, epochs: u32) -> Result<()>
where
    DB: Blockstore + Send + Sync,
{
    let epoch_limit = ts.epoch() - epochs as i64;
    let mut seen = CidHashSet::default();

    let pb = validation_spinner("Checking IPLD integrity:").with_finish(
        indicatif::ProgressFinish::AbandonWithMessage("❌ Invalid IPLD data!".into()),
    );

    for tipset in ts
        .chain(db)
        .take_while(|tipset| tipset.epoch() > epoch_limit)
    {
        let height = tipset.epoch();
        pb.set_message(format!("{} remaining epochs", height - epoch_limit));

        let mut assert_cid_exists = |cid: Cid| async move {
            let data = db.get(&cid)?;
            data.ok_or_else(|| anyhow::anyhow!("Broken IPLD link at epoch: {height}"))
        };

        for h in tipset.blocks() {
            recurse_links_hash(&mut seen, *h.state_root(), &mut assert_cid_exists, &|_| ()).await?;
            recurse_links_hash(&mut seen, *h.messages(), &mut assert_cid_exists, &|_| ()).await?;
        }
    }

    pb.finish_with_message("✅ verified!");
    Ok(())
}

// The genesis block determines the network identity (e.g., mainnet or
// calibnet). Scanning through the entire blockchain can be time-consuming, so
// Forest keeps a list of known tipsets for each network. Finding a known tipset
// short-circuits the search for the genesis block. If no genesis block can be
// found or if the genesis block is unrecognizable, an error is returned.
fn query_network(ts: &Tipset, db: impl Blockstore) -> Result<NetworkChain> {
    let pb = validation_spinner("Identifying genesis block:").with_finish(
        indicatif::ProgressFinish::AbandonWithMessage("✅ found!".into()),
    );

    fn match_genesis_block(block_cid: Cid) -> Result<NetworkChain> {
        if block_cid == *calibnet::GENESIS_CID {
            Ok(NetworkChain::Calibnet)
        } else if block_cid == *mainnet::GENESIS_CID {
            Ok(NetworkChain::Mainnet)
        } else {
            bail!("Unrecognizable genesis block");
        }
    }

    if let Ok(genesis_block) = ts.genesis(db) {
        return match_genesis_block(*genesis_block.cid());
    }

    pb.finish_with_message("❌ No valid genesis block!");
    bail!("Snapshot does not contain a genesis block")
}

// Each tipset in the blockchain contains a set of messages. A message is a
// transaction that manipulates a persistent state-tree. The hashes of these
// state-trees are stored in the tipsets and can be used to verify if the
// messages were correctly executed.
// Note: Messages may access state-trees 900 epochs in the past. So, if a
// snapshot has state-trees for 2000 epochs, one can only validate the messages
// for the last 1100 epochs.
async fn validate_stateroots<DB>(
    ts: Tipset,
    db: &Arc<DB>,
    network: NetworkChain,
    epochs: u32,
) -> Result<()>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let chain_config = Arc::new(ChainConfig::from_chain(&network));
    let genesis = ts.genesis(db)?;

    let pb = validation_spinner("Running tipset transactions:").with_finish(
        indicatif::ProgressFinish::AbandonWithMessage(
            "❌ Transaction result differs from Lotus!".into(),
        ),
    );

    let last_epoch = ts.epoch() - epochs as i64;

    // Bundles are required when doing state migrations.
    load_actor_bundles(&db).await?;

    // Set proof parameter data dir and make sure the proofs are available
    if cns::FETCH_PARAMS {
        crate::utils::proofs_api::paramfetch::set_proofs_parameter_cache_dir_env(
            &Config::default().client.data_dir,
        );
    }
    ensure_params_downloaded().await?;

    let chain_index = Arc::new(ChainIndex::new(Arc::new(db.clone())));

    // Prepare tipsets for validation
    let tipsets = chain_index
        .chain(Arc::new(ts))
        .take_while(|tipset| tipset.epoch() >= last_epoch)
        .inspect(|tipset| {
            pb.set_message(format!("epoch queue: {}", tipset.epoch() - last_epoch));
        });

    let beacon = Arc::new(chain_config.get_beacon_schedule(genesis.timestamp()));

    // ProgressBar::wrap_iter believes the progress has been abandoned once the
    // iterator is consumed.
    crate::state_manager::validate_tipsets(
        genesis.timestamp(),
        chain_index.clone(),
        chain_config,
        beacon,
        &MultiEngine::default(),
        tipsets,
    )?;

    pb.finish_with_message("✅ verified!");
    drop(pb);
    Ok(())
}

fn validation_spinner(prefix: &'static str) -> indicatif::ProgressBar {
    let pb = indicatif::ProgressBar::new_spinner()
        .with_style(
            indicatif::ProgressStyle::with_template("{spinner} {prefix:<30} {msg}")
                .expect("indicatif template must be valid"),
        )
        .with_prefix(prefix);
    pb.enable_steady_tick(std::time::Duration::from_secs_f32(0.1));
    pb
}

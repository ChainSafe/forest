use crate::blocks::TipsetKeys;
use crate::car_backed_blockstore::CarBackedBlockstore;
use crate::chain::ChainStore;
use crate::cli_shared::snapshot;
use crate::cli_shared::snapshot::TrustedVendor;
use crate::genesis::read_genesis_header;
use crate::shim::clock::ChainEpoch;
use crate::Config;
use anyhow::{bail, Context};
use chrono::Utc;
use clap::Subcommand;
use sha2::Sha256;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio_util::compat::TokioAsyncReadCompatExt;
use tracing::info;

#[derive(Debug, Subcommand)]
pub enum ArchiveCommands {
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
        /// dynamically from a config.
        // TODO: Investigate if we can have a dynamic default here somehow.
        #[arg(short)]
        depth: Option<ChainEpoch>,
    },
}

impl ArchiveCommands {
    pub async fn run(&self, config: Config) -> anyhow::Result<()> {
        match self {
            ArchiveCommands::Export {
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

                do_export(config, input_path, output_path, epoch, &depth).await
            }
        }
    }
}

async fn do_export(
    config: Config,
    input_path: &PathBuf,
    output_path: &PathBuf,
    epoch: &ChainEpoch,
    depth: &ChainEpoch,
) -> anyhow::Result<()> {
    info!(
        "indexing a car-backed store using snapshot: {}",
        input_path.to_str().unwrap_or_default()
    );

    let store = Arc::new(
        CarBackedBlockstore::new(std::fs::File::open(input_path)?)
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

    // TODO: This is totally unnecessary. It should be possible to do `tipset_by_height` without this step.
    // One solution to this is making `ts` an `Option` in `tipset_by_height` method.
    let ts = chain_store.tipset_from_keys(&TipsetKeys::new(chain_store.db.roots()))?;

    info!("looking up a tipset by epoch: {}", epoch);

    let ts = chain_store
        .tipset_by_height(*epoch, ts, true)
        .context("unable to get a tipset at given height")?;

    let output_path = match output_path.is_dir() {
        true => output_path.join(snapshot::filename(
            TrustedVendor::Forest,
            config.chain.network.to_string(),
            Utc::now().date_naive(),
            *epoch,
        )),
        false => output_path.clone(),
    };

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
        .export::<_, Sha256>(&ts, *depth, writer.compat(), true, true)
        .await?;

    Ok(())
}

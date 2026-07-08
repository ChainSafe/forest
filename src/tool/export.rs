// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Snapshot export logic shared between the `forest-tool archive` CLI
//! subcommands and the `Forest.ChainExportDiff` RPC method.

use std::path::PathBuf;

use anyhow::bail;
use chrono::DateTime;
use dialoguer::{Confirm, theme::ColorfulTheme};
use futures::TryStreamExt as _;
use indicatif::{ProgressBar, ProgressStyle};
use sha2::Sha256;

use crate::blocks::Tipset;
use crate::chain::{
    ChainEpochDelta, ExportOptions,
    index::{ChainIndex, ResolveNullTipset},
};
use crate::cid_collections::FileBackedCidHashSet;
use crate::cli_shared::{snapshot, snapshot::TrustedVendor};
use crate::db::DbImpl;
use crate::ipld::stream_chain;
use crate::networks::{ChainConfig, NetworkChain};
use crate::prelude::*;
use crate::shim::clock::EPOCH_DURATION_SECONDS;

// This does nothing if the output path is a file. If it is a directory - it produces the following:
// `./forest_snapshot_{chain}_{year}-{month}-{day}_height_{epoch}.car.zst`.
pub fn build_output_path(
    chain: String,
    genesis_timestamp: u64,
    epoch: ChainEpoch,
    output_path: PathBuf,
) -> PathBuf {
    match output_path.is_dir() {
        true => output_path.join(snapshot::filename(
            TrustedVendor::Forest,
            chain,
            DateTime::from_timestamp(genesis_timestamp as i64 + epoch * EPOCH_DURATION_SECONDS, 0)
                .unwrap_or_default()
                .naive_utc()
                .date(),
            epoch,
            true,
        )),
        false => output_path.clone(),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn do_export<DB>(
    store: &DB,
    root: Tipset,
    genesis: Option<Tipset>,
    output_path: PathBuf,
    epoch_option: Option<ChainEpoch>,
    depth: ChainEpochDelta,
    diff: Option<ChainEpoch>,
    diff_depth: Option<ChainEpochDelta>,
    force: bool,
) -> anyhow::Result<()>
where
    DB: Blockstore + ShallowClone + Into<DbImpl> + Unpin + Send + Sync + 'static,
{
    let ts = root;
    let genesis = if let Some(genesis) = genesis {
        genesis
    } else {
        // This is slow
        tracing::info!("Looking up genesis tipset...");
        ts.genesis(store.shallow_clone()).await?
    };
    let network =
        NetworkChain::from_genesis_or_devnet_placeholder(genesis.min_ticket_block().cid());

    let epoch = epoch_option.unwrap_or(ts.epoch());

    let finality = ChainConfig::from_chain(&network)
        .policy
        .chain_finality
        .min(epoch);
    if depth < finality {
        bail!("For {}, depth has to be at least {}.", network, finality);
    }

    info!("looking up a tipset by epoch: {}", epoch);

    let index = ChainIndex::new(store.shallow_clone(), genesis.shallow_clone());

    let ts = index
        .load_required_tipset_by_height(epoch, ts, ResolveNullTipset::TakeOlder)
        .await
        .context("unable to get a tipset at given height")?;

    let seen = if let Some(diff) = diff {
        let diff_ts: Tipset = index
            .load_required_tipset_by_height(diff, ts.shallow_clone(), ResolveNullTipset::TakeOlder)
            .await
            .context("diff epoch must be smaller than target epoch")?;
        let diff_ts: &Tipset = &diff_ts;
        let diff_limit = diff_depth.map(|depth| diff_ts.epoch() - depth).unwrap_or(0);
        let store = Arc::new(store.shallow_clone());
        let mut stream = stream_chain(
            store.shallow_clone(),
            diff_ts.clone().chain_owned(store.shallow_clone()),
            diff_limit,
            FileBackedCidHashSet::new_in_temp_dir()?,
        );
        while stream.try_next().await?.is_some() {}
        stream.into_seen()
    } else {
        FileBackedCidHashSet::new_in_temp_dir()?
    };

    let output_path = build_output_path(
        network.to_string(),
        genesis.min_ticket_block().timestamp,
        epoch,
        output_path,
    );

    if !force && output_path.exists() {
        let have_permission = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "{} will be overwritten. Continue?",
                output_path.to_string_lossy()
            ))
            .default(false)
            .interact()
            // e.g not a tty (or some other error), so haven't got permission.
            .unwrap_or(false);
        if !have_permission {
            return Ok(());
        }
    }

    let writer = tokio::fs::File::create(&output_path)
        .await
        .with_context(|| {
            format!(
                "unable to create a snapshot - is the output path '{}' correct?",
                output_path.to_str().unwrap_or_default()
            )
        })?;

    info!(
        "exporting snapshot at location: {}",
        output_path.to_str().unwrap_or_default()
    );

    let pb = ProgressBar::new_spinner().with_style(
        ProgressStyle::with_template(
            "{spinner} exported {total_bytes} with {binary_bytes_per_sec} in {elapsed}",
        )
        .expect("indicatif template must be valid"),
    );
    pb.enable_steady_tick(std::time::Duration::from_secs_f32(0.1));
    let writer = pb.wrap_async_write(writer);

    crate::chain::export::<Sha256, _>(
        store,
        &ts,
        depth,
        writer,
        ExportOptions {
            skip_checksum: true,
            include_receipts: false,
            include_events: false,
            include_tipset_keys: false,
            seen,
        },
    )
    .await?;

    Ok(())
}

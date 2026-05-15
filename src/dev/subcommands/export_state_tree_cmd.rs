// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    chain::{ChainStore, index::ResolveNullTipset},
    cid_collections::FileBackedCidHashSet,
    cli_shared::{chain_path, read_config},
    daemon::db_util::load_all_forest_cars,
    db::{
        CAR_DB_DIR_NAME,
        car::{ManyCar, forest::FOREST_CAR_FILE_EXTENSION},
        db_engine::{db_root, open_db},
    },
    genesis::read_genesis_header,
    ipld::IpldStream,
    networks::{ChainConfig, NetworkChain},
    shim::{clock::ChainEpoch, executor::Receipt},
};
use anyhow::Context as _;
use clap::Args;
use itertools::Itertools;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::io::AsyncWriteExt as _;

/// Exports N consecutive parent state trees(together with messages, message receipts and events) of the tipset at the given epoch
#[derive(Debug, Args)]
pub struct ExportStateTreeCommand {
    /// Filecoin network chain (e.g., calibnet, mainnet)
    #[arg(long, required = true)]
    chain: NetworkChain,
    /// Optional path to the database folder
    #[arg(long)]
    db: Option<PathBuf>,
    /// The maximum tipset epoch to export state tree from (Exclusive)
    #[arg(long)]
    from: ChainEpoch,
    /// The minimum tipset epoch to export state tree from (Inclusive)
    #[arg(long)]
    to: ChainEpoch,
    /// The path to the output `ForestCAR` file
    #[arg(short, long)]
    output: Option<PathBuf>,
}

impl ExportStateTreeCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let Self {
            chain,
            db,
            from,
            to,
            output,
        } = self;
        let output = output.unwrap_or_else(|| {
            Path::new(&format!(
                "statetree_{chain}_{to}_{from}{FOREST_CAR_FILE_EXTENSION}"
            ))
            .to_owned()
        });
        let db_root_path = if let Some(db) = db {
            db
        } else {
            let (_, config) = read_config(None, Some(chain.clone()))?;
            db_root(&chain_path(&config))?
        };
        let forest_car_db_dir = db_root_path.join(CAR_DB_DIR_NAME);
        let db: Arc<ManyCar<crate::db::parity_db::ParityDb>> =
            Arc::new(ManyCar::new(open_db(db_root_path, &Default::default())?));
        load_all_forest_cars(&db, &forest_car_db_dir)?;

        let chain_config = Arc::new(ChainConfig::from_chain(&chain));
        let genesis_header =
            read_genesis_header(None, chain_config.genesis_bytes(&db).await?.as_deref(), &db)
                .await?;
        let chain_store = ChainStore::new(db.clone(), chain_config, genesis_header)?;

        let start_ts = chain_store.chain_index().load_required_tipset_by_height(
            from,
            chain_store.heaviest_tipset(),
            ResolveNullTipset::TakeNewer,
        )?;

        let mut ipld_roots = vec![];
        for (child, ts) in start_ts
            .chain(&db)
            .tuple_windows()
            .take_while(|(_, parent)| parent.epoch() >= to)
        {
            ipld_roots.extend([*child.parent_state(), *child.parent_message_receipts()]);
            ipld_roots.extend(ts.block_headers().iter().map(|h| h.messages));
            let receipts = Receipt::get_receipts(&db, *child.parent_message_receipts())
                .with_context(|| {
                    format!(
                        "failed to get receipts, root: {}, epoch: {}, tipset key: {}",
                        *child.parent_message_receipts(),
                        ts.epoch(),
                        ts.key(),
                    )
                })?;
            ipld_roots.extend(receipts.into_iter().filter_map(|r| r.events_root()));
        }
        let roots = nunny::vec![ipld_roots.first().cloned().context("no ipld roots found")?];
        let stream = IpldStream::new(
            db,
            ipld_roots.clone(),
            FileBackedCidHashSet::new_in_temp_dir()?,
        );
        let frames = crate::db::car::forest::Encoder::compress_stream_default(stream);
        let tmp =
            tempfile::NamedTempFile::new_in(output.parent().unwrap_or_else(|| Path::new(".")))?
                .into_temp_path();
        let mut writer = tokio::io::BufWriter::new(tokio::fs::File::create(&tmp).await?);
        crate::db::car::forest::Encoder::write(&mut writer, roots, frames).await?;
        writer.flush().await?;
        tmp.persist(output)?;

        Ok(())
    }
}

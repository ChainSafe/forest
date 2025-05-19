// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use cid::Cid;
use clap::Args;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;

use crate::db::BlockstoreWithWriteBuffer;
use crate::utils::db::CborStoreExt;
use crate::{
    blocks::CachingBlockHeader,
    cli_shared::{chain_path, read_config},
    daemon::db_util::load_all_forest_cars,
    db::{
        CAR_DB_DIR_NAME,
        car::ManyCar,
        db_engine::{db_root, open_db},
        parity_db::ParityDb,
    },
    networks::{ChainConfig, NetworkChain},
    shim::version::NetworkVersion,
};

#[derive(Debug, Args)]
pub struct MigrateStateCommand {
    /// Target network version
    network_version: NetworkVersion,
    /// Block to look back from
    block_to_look_back: Cid,
    /// Path to the Forest database folder
    #[arg(long)]
    db: Option<PathBuf>,
    /// Filecoin network chain
    #[arg(long, required = true)]
    chain: NetworkChain,
    /// Size of database write buffer, use 0 to disable write buffer
    #[arg(long, default_value_t = 10000)]
    db_write_buffer: usize,
}

impl MigrateStateCommand {
    pub async fn run(self, _: crate::rpc::Client) -> anyhow::Result<()> {
        let Self {
            network_version,
            block_to_look_back,
            db,
            chain,
            db_write_buffer,
        } = self;
        let db = {
            let db = if let Some(db) = db {
                db
            } else {
                let (_, config) = read_config(None, Some(chain.clone()))?;
                db_root(&chain_path(&config))?
            };
            let db = load_db(&db)?;
            Arc::new(if db_write_buffer > 0 {
                Either::Left(BlockstoreWithWriteBuffer::new_with_capacity(
                    db,
                    db_write_buffer,
                ))
            } else {
                Either::Right(db)
            })
        };
        let block: CachingBlockHeader = db.get_cbor_required(&block_to_look_back)?;
        let chain_config = Arc::new(ChainConfig::from_chain(&chain));
        let mut state_root = block.state_root;
        let epoch = block.epoch - 1;
        let migrations = crate::state_migration::get_migrations(&chain)
            .into_iter()
            .filter(|(h, _)| {
                let nv: NetworkVersion = (*h).into();
                network_version == nv
            })
            .collect_vec();
        anyhow::ensure!(
            !migrations.is_empty(),
            "No migration found for network version {network_version} on {chain}"
        );
        for (_, migrate) in migrations {
            println!("Migrating... state_root: {state_root}, epoch: {epoch}");
            let start = Instant::now();
            let new_state = migrate(&chain_config, &db, &state_root, epoch)?;
            println!(
                "Done. old_state: {state_root}, new_state: {new_state}, took: {}",
                humantime::format_duration(start.elapsed())
            );
            state_root = new_state;
        }
        Ok(())
    }
}

pub(super) fn load_db(db_root: &Path) -> anyhow::Result<Arc<ManyCar<ParityDb>>> {
    let db_writer = open_db(db_root.into(), Default::default())?;
    let db = ManyCar::new(db_writer);
    let forest_car_db_dir = db_root.join(CAR_DB_DIR_NAME);
    load_all_forest_cars(&db, &forest_car_db_dir)?;
    Ok(Arc::new(db))
}

enum Either<A: Blockstore, B: Blockstore> {
    Left(A),
    Right(B),
}

impl<A: Blockstore, B: Blockstore> Blockstore for Either<A, B> {
    fn has(&self, k: &Cid) -> anyhow::Result<bool> {
        match self {
            Self::Left(v) => v.has(k),
            Self::Right(v) => v.has(k),
        }
    }

    #[allow(clippy::disallowed_types)]
    fn put<D>(
        &self,
        mh_code: multihash_codetable::Code,
        block: &fvm_ipld_blockstore::Block<D>,
    ) -> anyhow::Result<Cid>
    where
        Self: Sized,
        D: AsRef<[u8]>,
    {
        match self {
            Self::Left(v) => v.put(mh_code, block),
            Self::Right(v) => v.put(mh_code, block),
        }
    }

    #[allow(clippy::disallowed_types)]
    fn put_many<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (multihash_codetable::Code, fvm_ipld_blockstore::Block<D>)>,
    {
        match self {
            Self::Left(v) => v.put_many(blocks),
            Self::Right(v) => v.put_many(blocks),
        }
    }

    fn put_many_keyed<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (Cid, D)>,
    {
        match self {
            Self::Left(v) => v.put_many_keyed(blocks),
            Self::Right(v) => v.put_many_keyed(blocks),
        }
    }

    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        match self {
            Self::Left(v) => v.get(k),
            Self::Right(v) => v.get(k),
        }
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        match self {
            Self::Left(v) => v.put_keyed(k, block),
            Self::Right(v) => v.put_keyed(k, block),
        }
    }
}

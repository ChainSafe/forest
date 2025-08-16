// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::CachingBlockHeader;
use anyhow::Result;
use parity_db::{Db, Options};

pub struct SlasherDb {
    db: Db,
}

pub enum SlasherDbColumns {
    ByEpoch = 0,
    ByParents = 1,
}

impl SlasherDb {
    pub fn new(data_dir: std::path::PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&data_dir)?;

        let mut options = Options::with_columns(&data_dir, 2);
        if let Some(column) = options.columns.get_mut(SlasherDbColumns::ByEpoch as usize) {
            column.btree_index = true;
            column.uniform = false;
        }
        if let Some(column) = options
            .columns
            .get_mut(SlasherDbColumns::ByParents as usize)
        {
            column.btree_index = true;
            column.uniform = false;
        }

        let db = Db::open_or_create(&options)?;

        Ok(Self { db })
    }

    pub fn put(&mut self, header: &CachingBlockHeader) -> Result<()> {
        let miner = header.miner_address;
        let epoch = header.epoch;

        let epoch_key = format!("{}/{}", miner, epoch);
        let parent_key = format!("{}/{}", miner, header.parents);

        self.db.commit(vec![
            (
                SlasherDbColumns::ByEpoch as u8,
                epoch_key.as_bytes(),
                Some(header.cid().to_bytes()),
            ),
            (
                SlasherDbColumns::ByParents as u8,
                parent_key.as_bytes(),
                Some(header.cid().to_bytes()),
            ),
        ])?;

        Ok(())
    }

    pub fn get(&self, column: u8, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.db.get(column, key)?)
    }
}

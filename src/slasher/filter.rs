// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::CachingBlockHeader;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::slasher::types::*;
use anyhow::Result;
use parity_db::{Db, Options};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BlockInfo {
    miner_address: Vec<u8>,
    epoch: ChainEpoch,
    parents: crate::blocks::TipsetKey,
    cid: cid::Cid,
}

pub struct SlasherFilter {
    db: Db,
}

impl SlasherFilter {
    pub fn new(data_dir: std::path::PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&data_dir)?;

        let mut options = Options::with_columns(&data_dir, 1);
        if let Some(column) = options.columns.get_mut(0) {
            column.btree_index = true;
            column.uniform = false;
        }

        let db = Db::open_or_create(&options)?;

        Ok(Self { db })
    }

    pub fn process_block(&mut self, header: &CachingBlockHeader) -> Result<Option<ConsensusFault>> {
        self.add_to_history(header)?;
        let fault = self.check_consensus_faults(header)?;

        Ok(fault)
    }

    fn add_to_history(&mut self, header: &CachingBlockHeader) -> Result<()> {
        let miner = header.miner_address;
        let epoch = header.epoch;

        let block_info = BlockInfo {
            miner_address: miner.to_bytes(),
            epoch,
            parents: header.parents.clone(),
            cid: *header.cid(),
        };

        let key = self.create_db_key(miner, epoch, header.cid());
        let value = serde_json::to_vec(&block_info)?;

        self.db.commit(vec![(0, key, Some(value))])?;

        Ok(())
    }

    fn create_db_key(&self, miner: Address, epoch: ChainEpoch, cid: &cid::Cid) -> Vec<u8> {
        let mut key = Vec::new();
        key.extend_from_slice(&miner.to_bytes());
        key.extend_from_slice(&epoch.to_le_bytes());
        key.extend_from_slice(cid.to_bytes().as_slice());
        key
    }

    fn load_blocks_from_db(&mut self, miner: Address, epoch: ChainEpoch) -> Result<Vec<BlockInfo>> {
        let mut blocks = Vec::new();
        let prefix = self.create_db_key_prefix(miner, epoch);

        let mut iter = self.db.iter(0)?;
        while let Some((key, value)) = iter.next()? {
            if key.starts_with(&prefix) {
                if let Ok(block_info) = serde_json::from_slice::<BlockInfo>(&value) {
                    blocks.push(block_info);
                }
            }
        }

        Ok(blocks)
    }

    fn create_db_key_prefix(&self, miner: Address, epoch: ChainEpoch) -> Vec<u8> {
        let mut prefix = Vec::new();
        prefix.extend_from_slice(&miner.to_bytes());
        prefix.extend_from_slice(&epoch.to_le_bytes());
        prefix
    }

    fn check_consensus_faults(
        &mut self,
        header: &CachingBlockHeader,
    ) -> Result<Option<ConsensusFault>> {
        if let Some(fault) = self.check_double_fork_mining(header)? {
            return Ok(Some(fault));
        }

        if let Some(fault) = self.check_time_offset_mining(header)? {
            return Ok(Some(fault));
        }

        if let Some(fault) = self.check_parent_grinding(header)? {
            return Ok(Some(fault));
        }

        Ok(None)
    }

    fn check_double_fork_mining(
        &mut self,
        header: &CachingBlockHeader,
    ) -> Result<Option<ConsensusFault>> {
        let miner = header.miner_address;
        let epoch = header.epoch;

        let blocks = self.load_blocks_from_db(miner, epoch)?;

        // If we have more than one block from this miner at this epoch, it's double-fork mining
        if blocks.len() > 1 {
            let block_headers: Vec<cid::Cid> = blocks.iter().map(|b| b.cid).collect();

            return Ok(Some(ConsensusFault {
                miner_address: miner,
                detection_epoch: epoch,
                fault_type: ConsensusFaultType::DoubleForkMining,
                block_headers,
                extra_evidence: None,
            }));
        }

        Ok(None)
    }

    fn check_time_offset_mining(
        &mut self,
        header: &CachingBlockHeader,
    ) -> Result<Option<ConsensusFault>> {
        let miner = header.miner_address;
        let current_cid = *header.cid();

        let mut iter = self.db.iter(0)?;
        let mut same_parent_blocks = Vec::new();

        while let Some((_, value)) = iter.next()? {
            if let Ok(block_info) = serde_json::from_slice::<BlockInfo>(&value) {
                if block_info.parents == header.parents && block_info.cid != current_cid {
                    if let Ok(block_miner) = Address::from_bytes(&block_info.miner_address) {
                        if block_miner == miner {
                            same_parent_blocks.push(block_info);
                        }
                    }
                }
            }
        }

        if !same_parent_blocks.is_empty() {
            let mut block_headers = vec![current_cid];
            block_headers.extend(same_parent_blocks.iter().map(|b| b.cid));

            return Ok(Some(ConsensusFault {
                miner_address: miner,
                detection_epoch: header.epoch,
                fault_type: ConsensusFaultType::TimeOffsetMining,
                block_headers,
                extra_evidence: None,
            }));
        }

        Ok(None)
    }

    fn check_parent_grinding(
        &mut self,
        header: &CachingBlockHeader,
    ) -> Result<Option<ConsensusFault>> {
        let miner = header.miner_address;
        let current_epoch = header.epoch;

        if current_epoch < 1 {
            return Ok(None);
        }

        let prev_blocks = self.load_blocks_from_db(miner, current_epoch - 1)?;

        for prev_block in prev_blocks {
            if header.parents.contains(prev_block.cid) {
                continue;
            }

            if let Some(witness) = self.find_parent_grinding_witness(header, &prev_block)? {
                return Ok(Some(ConsensusFault {
                    miner_address: miner,
                    detection_epoch: current_epoch,
                    fault_type: ConsensusFaultType::ParentGrinding,
                    block_headers: vec![prev_block.cid, *header.cid()],
                    extra_evidence: Some(witness.cid),
                }));
            }
        }

        Ok(None)
    }

    fn find_parent_grinding_witness(
        &self,
        current_block: &CachingBlockHeader,
        miner_prev_block: &BlockInfo,
    ) -> Result<Option<BlockInfo>> {
        let prev_epoch = current_block.epoch - 1;

        let mut iter = self.db.iter(0)?;
        while let Some((_, value)) = iter.next()? {
            if let Ok(block_info) = serde_json::from_slice::<BlockInfo>(&value) {
                if block_info.epoch == prev_epoch
                    && block_info.parents == miner_prev_block.parents
                    && block_info.cid != miner_prev_block.cid
                    && current_block.parents.contains(block_info.cid)
                {
                    return Ok(Some(block_info));
                }
            }
        }

        Ok(None)
    }
}

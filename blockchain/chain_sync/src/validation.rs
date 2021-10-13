// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::bad_block_cache::BadBlockCache;

use amt::{Amt, Error as IpldAmtError};
use blocks::{Block, FullTipset, Tipset, TxMeta};
use chain::ChainStore;
use cid::{Cid, Code::Blake2b256};
use encoding::{Cbor, Error as EncodingError};
use ipld_blockstore::BlockStore;
use message::{SignedMessage, UnsignedMessage};
use networks::BLOCK_DELAY_SECS;

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;

const MAX_HEIGHT_DRIFT: u64 = 5;

#[derive(Debug, Error)]
pub enum TipsetValidationError {
    #[error("Tipset has no blocks")]
    NoBlocks,
    #[error("Tipset has an epoch that is too large")]
    EpochTooLarge,
    #[error("Tipset has an insufficient weight")]
    InsufficientWeight,
    #[error("Tipset block = [CID = {0}] is invalid: {1}")]
    InvalidBlock(Cid, String),
    #[error("Tipset headers are invalid")]
    InvalidRoots,
    #[error("Tipset IPLD error: {0}")]
    IpldAmt(String),
    #[error("Block store error while validating tipset: {0}")]
    Blockstore(String),
    #[error("Encoding error while validating tipset: {0}")]
    Encoding(EncodingError),
}

impl From<IpldAmtError> for TipsetValidationError {
    fn from(err: IpldAmtError) -> Self {
        Self::IpldAmt(err.to_string())
    }
}

impl From<EncodingError> for TipsetValidationError {
    fn from(err: EncodingError) -> Self {
        Self::Encoding(err)
    }
}

pub struct TipsetValidator<'a>(pub &'a FullTipset);

impl<'a> TipsetValidator<'a> {
    pub async fn validate<DB: BlockStore + Send + Sync + 'static>(
        &self,
        chainstore: Arc<ChainStore<DB>>,
        bad_block_cache: Arc<BadBlockCache>,
        genesis_tipset: Arc<Tipset>,
    ) -> Result<(), TipsetValidationError> {
        // No empty blocks
        if self.0.blocks().is_empty() {
            return Err(TipsetValidationError::NoBlocks);
        }

        // Tipset epoch must not be behind current max
        self.validate_epoch(genesis_tipset)?;

        // Validate each block in the tipset by:
        // 1. Calculating the message root using all of the messages to ensure it matches the mst root in the block header
        // 2. Ensuring it has not previously been seen in the bad blocks cache
        for block in self.0.blocks() {
            self.validate_msg_root(chainstore.db.as_ref(), block)?;
            if let Some(bad) = bad_block_cache.peek(block.cid()).await {
                return Err(TipsetValidationError::InvalidBlock(*block.cid(), bad));
            }
        }

        Ok(())
    }

    pub fn validate_epoch(&self, genesis_tipset: Arc<Tipset>) -> Result<(), TipsetValidationError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let max_epoch =
            ((now - genesis_tipset.min_timestamp()) / BLOCK_DELAY_SECS) + MAX_HEIGHT_DRIFT;
        let too_far_ahead_in_time = self.0.epoch() as u64 > max_epoch;
        if too_far_ahead_in_time {
            Err(TipsetValidationError::EpochTooLarge)
        } else {
            Ok(())
        }
    }

    pub fn validate_msg_root<DB: BlockStore>(
        &self,
        blockstore: &DB,
        block: &Block,
    ) -> Result<(), TipsetValidationError> {
        let msg_root = Self::compute_msg_root(blockstore, block.bls_msgs(), block.secp_msgs())?;
        if block.header().messages() != &msg_root {
            Err(TipsetValidationError::InvalidRoots)
        } else {
            Ok(())
        }
    }

    pub fn compute_msg_root<DB: BlockStore>(
        blockstore: &DB,
        bls_msgs: &[UnsignedMessage],
        secp_msgs: &[SignedMessage],
    ) -> Result<Cid, TipsetValidationError> {
        // Generate message CIDs
        let bls_cids = bls_msgs
            .iter()
            .map(Cbor::cid)
            .collect::<Result<Vec<Cid>, EncodingError>>()?;
        let secp_cids = secp_msgs
            .iter()
            .map(Cbor::cid)
            .collect::<Result<Vec<Cid>, EncodingError>>()?;

        // Generate Amt and batch set message values
        let bls_message_root = Amt::new_from_iter(blockstore, bls_cids)?;
        let secp_message_root = Amt::new_from_iter(blockstore, secp_cids)?;
        let meta = TxMeta {
            bls_message_root,
            secp_message_root,
        };

        // Store message roots and receive meta_root CID
        blockstore
            .put(&meta, Blake2b256)
            .map_err(|e| TipsetValidationError::Blockstore(e.to_string()))
    }
}

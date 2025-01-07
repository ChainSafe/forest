// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::{SystemTime, UNIX_EPOCH};

use crate::blocks::{Block, FullTipset, Tipset, TxMeta};
use crate::chain::ChainStore;
use crate::message::SignedMessage;
use crate::shim::message::Message;
use crate::utils::{cid::CidCborExt, db::CborStoreExt};
use cid::Cid;
use fil_actors_shared::fvm_ipld_amt::{Amtv0 as Amt, Error as IpldAmtError};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::Error as EncodingError;
use thiserror::Error;

use crate::chain_sync::bad_block_cache::BadBlockCache;

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

impl From<EncodingError> for TipsetValidationError {
    fn from(err: EncodingError) -> Self {
        TipsetValidationError::Encoding(err)
    }
}

impl From<IpldAmtError> for TipsetValidationError {
    fn from(err: IpldAmtError) -> Self {
        TipsetValidationError::IpldAmt(err.to_string())
    }
}

pub struct TipsetValidator<'a>(pub &'a FullTipset);

impl TipsetValidator<'_> {
    pub fn validate<DB: Blockstore>(
        &self,
        chainstore: &ChainStore<DB>,
        bad_block_cache: Option<&BadBlockCache>,
        genesis_tipset: &Tipset,
        block_delay: u32,
    ) -> Result<(), TipsetValidationError> {
        // No empty blocks
        if self.0.blocks().is_empty() {
            return Err(TipsetValidationError::NoBlocks);
        }

        // Tipset epoch must not be behind current max
        self.validate_epoch(genesis_tipset, block_delay)?;

        // Validate each block in the tipset by:
        // 1. Calculating the message root using all of the messages to ensure it
        // matches the mst root in the block header 2. Ensuring it has not
        // previously been seen in the bad blocks cache
        for block in self.0.blocks() {
            self.validate_msg_root(&chainstore.db, block)?;
            if let Some(bad_block_cache) = bad_block_cache {
                if let Some(bad) = bad_block_cache.peek(block.cid()) {
                    return Err(TipsetValidationError::InvalidBlock(*block.cid(), bad));
                }
            }
        }

        Ok(())
    }

    pub fn validate_epoch(
        &self,
        genesis_tipset: &Tipset,
        block_delay: u32,
    ) -> Result<(), TipsetValidationError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let max_epoch =
            ((now - genesis_tipset.min_timestamp()) / block_delay as u64) + MAX_HEIGHT_DRIFT;
        let too_far_ahead_in_time = self.0.epoch() as u64 > max_epoch;
        if too_far_ahead_in_time {
            Err(TipsetValidationError::EpochTooLarge)
        } else {
            Ok(())
        }
    }

    pub fn validate_msg_root<DB: Blockstore>(
        &self,
        blockstore: &DB,
        block: &Block,
    ) -> Result<(), TipsetValidationError> {
        let msg_root = Self::compute_msg_root(blockstore, block.bls_msgs(), block.secp_msgs())?;
        if block.header().messages != msg_root {
            Err(TipsetValidationError::InvalidRoots)
        } else {
            Ok(())
        }
    }

    pub fn compute_msg_root<DB: Blockstore>(
        blockstore: &DB,
        bls_msgs: &[Message],
        secp_msgs: &[SignedMessage],
    ) -> Result<Cid, TipsetValidationError> {
        // Generate message CIDs
        let bls_cids = bls_msgs
            .iter()
            .map(Cid::from_cbor_blake2b256)
            .collect::<Result<Vec<Cid>, fvm_ipld_encoding::Error>>()?;
        let secp_cids = secp_msgs
            .iter()
            .map(Cid::from_cbor_blake2b256)
            .collect::<Result<Vec<Cid>, fvm_ipld_encoding::Error>>()?;

        // Generate Amt and batch set message values
        let bls_message_root = Amt::new_from_iter(blockstore, bls_cids)?;
        let secp_message_root = Amt::new_from_iter(blockstore, secp_cids)?;
        let meta = TxMeta {
            bls_message_root,
            secp_message_root,
        };

        // Store message roots and receive meta_root CID
        blockstore
            .put_cbor_default(&meta)
            .map_err(|e| TipsetValidationError::Blockstore(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use crate::db::MemoryDB;
    use crate::message::SignedMessage;
    use crate::shim::message::Message;
    use crate::test_utils::construct_messages;
    use crate::utils::encoding::from_slice_with_fallback;
    use base64::{prelude::BASE64_STANDARD, Engine};
    use cid::Cid;

    use super::TipsetValidator;

    #[test]
    fn compute_msg_meta_given_msgs_test() {
        let blockstore = MemoryDB::default();

        let (bls, secp) = construct_messages();

        let expected_root =
            Cid::try_from("bafy2bzaceasssikoiintnok7f3sgnekfifarzobyr3r4f25sgxmn23q4c35ic")
                .unwrap();

        let root = TipsetValidator::compute_msg_root(&blockstore, &[bls], &[secp])
            .expect("Computing message root should succeed");
        assert_eq!(root, expected_root);
    }

    #[test]
    fn empty_msg_meta_vector() {
        let blockstore = MemoryDB::default();
        let usm: Vec<Message> =
            from_slice_with_fallback(&BASE64_STANDARD.decode("gA==").unwrap()).unwrap();
        let sm: Vec<SignedMessage> =
            from_slice_with_fallback(&BASE64_STANDARD.decode("gA==").unwrap()).unwrap();

        assert_eq!(
            TipsetValidator::compute_msg_root(&blockstore, &usm, &sm)
                .expect("Computing message root should succeed")
                .to_string(),
            "bafy2bzacecmda75ovposbdateg7eyhwij65zklgyijgcjwynlklmqazpwlhba"
        );
    }
}

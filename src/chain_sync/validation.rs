// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::{SystemTime, UNIX_EPOCH};

use crate::blocks::{BLOCK_MESSAGE_LIMIT, Block, FullTipset, GossipBlock, Tipset, TxMeta};
use crate::chain::ChainStore;
use crate::message::SignedMessage;
use crate::shim::clock::ChainEpoch;
use crate::shim::message::Message;
use crate::utils::{cid::CidCborExt, db::CborStoreExt};
use cid::Cid;
use fil_actors_shared::fvm_ipld_amt::{Amtv0 as Amt, Error as IpldAmtError};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::Error as EncodingError;
use thiserror::Error;

use crate::chain_sync::bad_block_cache::{BadBlockCache, SeenBlockCache};

const MAX_HEIGHT_DRIFT: ChainEpoch = 5;

/// Compute the maximum allowed epoch given the current time (seconds since
/// UNIX epoch). Returns `None` if inputs are nonsensical (clock before
/// genesis, zero block delay).
fn max_allowed_epoch(
    now_secs: u64,
    genesis_timestamp: u64,
    block_delay: u32,
) -> Option<ChainEpoch> {
    let elapsed = now_secs.checked_sub(genesis_timestamp)?;
    let delay = u64::from(block_delay);
    if delay == 0 {
        return None;
    }
    let epoch = ChainEpoch::try_from(elapsed / delay).unwrap_or(ChainEpoch::MAX);
    Some(epoch.saturating_add(MAX_HEIGHT_DRIFT))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Error)]
pub enum TipsetValidationError {
    #[error("Tipset has no blocks")]
    NoBlocks,
    #[error("Tipset has an epoch that is too large")]
    EpochTooLarge,
    #[error("Tipset has an insufficient weight")]
    InsufficientWeight,
    #[error("Tipset block = [CID = {0}] is invalid")]
    InvalidBlock(Cid),
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
    pub fn validate(
        &self,
        chainstore: &ChainStore,
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
            Self::validate_msg_root(chainstore.db(), block)?;
            if let Some(bad_block_cache) = bad_block_cache
                && bad_block_cache.peek(block.cid()).is_some()
            {
                return Err(TipsetValidationError::InvalidBlock(*block.cid()));
            }
        }

        Ok(())
    }

    pub fn validate_epoch(
        &self,
        genesis_tipset: &Tipset,
        block_delay: u32,
    ) -> Result<(), TipsetValidationError> {
        let max = max_allowed_epoch(now_secs(), genesis_tipset.min_timestamp(), block_delay)
            .unwrap_or(ChainEpoch::MAX);
        if self.0.epoch() > max {
            Err(TipsetValidationError::EpochTooLarge)
        } else {
            Ok(())
        }
    }

    pub fn validate_msg_root<DB: Blockstore>(
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

#[derive(Debug, Error)]
pub enum GossipBlockRejectReason {
    #[error("block epoch {0} is too far in the future")]
    EpochTooFarAhead(ChainEpoch),
    #[error("block epoch {0} is beyond finality (heaviest: {1})")]
    EpochBeyondFinality(ChainEpoch, ChainEpoch),
    #[error("block epoch {0} is negative")]
    NegativeEpoch(ChainEpoch),
    #[error("block timestamp {timestamp} inconsistent with epoch {epoch} (expected {expected})")]
    TimestampMismatch {
        timestamp: u64,
        epoch: ChainEpoch,
        expected: u64,
    },
    #[error("block has no signature")]
    MissingSignature,
    #[error("block has no election proof")]
    MissingElectionProof,
    #[error("block election proof has win_count {0} < 1")]
    InvalidWinCount(i64),
    #[error("block has {0} messages, exceeding limit of {BLOCK_MESSAGE_LIMIT}")]
    TooManyMessages(usize),
    #[error("block CID {0} is in bad block cache")]
    BadBlock(Cid),
    #[error("duplicate block CID {0}")]
    DuplicateBlock(Cid),
}

impl GossipBlockRejectReason {
    pub fn label(&self) -> &'static str {
        match self {
            Self::EpochTooFarAhead(_) => "epoch_too_far_ahead",
            Self::EpochBeyondFinality(_, _) => "epoch_beyond_finality",
            Self::NegativeEpoch(_) => "negative_epoch",
            Self::TimestampMismatch { .. } => "timestamp_mismatch",
            Self::MissingSignature => "missing_signature",
            Self::MissingElectionProof => "missing_election_proof",
            Self::InvalidWinCount(_) => "invalid_win_count",
            Self::TooManyMessages(_) => "too_many_messages",
            Self::BadBlock(_) => "bad_block",
            Self::DuplicateBlock(_) => "duplicate_block",
        }
    }
}

/// Pre-validation of gossip blocks to avoid expensive `get_full_tipset`
/// network round-trips and DB writes for obviously invalid blocks.
/// Only uses data already present in the gossip message (header + CIDs).
pub struct GossipBlockValidator<'a> {
    block: &'a GossipBlock,
}

impl<'a> GossipBlockValidator<'a> {
    pub fn new(block: &'a GossipBlock) -> Self {
        Self { block }
    }

    /// Run all pre-fetch validation checks.
    /// Checks are ordered cheapest/most-likely-to-reject first.
    pub fn validate_pre_fetch(
        &self,
        genesis_tipset: &Tipset,
        block_delay: u32,
        chain_finality: ChainEpoch,
        heaviest_epoch: ChainEpoch,
        bad_block_cache: Option<&BadBlockCache>,
        seen_block_cache: &SeenBlockCache,
    ) -> Result<(), GossipBlockRejectReason> {
        let cid = *self.block.header.cid();
        Self::check_bad_block_cache(cid, bad_block_cache)?;
        self.validate_epoch_range(genesis_tipset, block_delay, chain_finality, heaviest_epoch)?;
        self.validate_timestamp(genesis_tipset, block_delay)?;
        self.validate_election_proof()?;
        self.validate_signature_present()?;
        self.validate_message_count()?;
        // Insert into seen cache only after all checks pass, so transiently
        // rejected blocks (e.g., slightly-future epoch) aren't suppressed later.
        Self::check_duplicate(cid, seen_block_cache)?;
        Ok(())
    }

    fn check_duplicate(
        cid: Cid,
        seen_block_cache: &SeenBlockCache,
    ) -> Result<(), GossipBlockRejectReason> {
        if seen_block_cache.test_and_insert(&cid) {
            return Err(GossipBlockRejectReason::DuplicateBlock(cid));
        }
        Ok(())
    }

    fn check_bad_block_cache(
        cid: Cid,
        bad_block_cache: Option<&BadBlockCache>,
    ) -> Result<(), GossipBlockRejectReason> {
        if let Some(cache) = bad_block_cache
            && cache.peek(&cid).is_some()
        {
            return Err(GossipBlockRejectReason::BadBlock(cid));
        }
        Ok(())
    }

    fn validate_epoch_range(
        &self,
        genesis_tipset: &Tipset,
        block_delay: u32,
        chain_finality: ChainEpoch,
        heaviest_epoch: ChainEpoch,
    ) -> Result<(), GossipBlockRejectReason> {
        let epoch = self.block.header.epoch;
        if epoch < 0 {
            return Err(GossipBlockRejectReason::NegativeEpoch(epoch));
        }
        let max = max_allowed_epoch(now_secs(), genesis_tipset.min_timestamp(), block_delay)
            .unwrap_or(ChainEpoch::MAX);
        if epoch > max {
            return Err(GossipBlockRejectReason::EpochTooFarAhead(epoch));
        }
        if heaviest_epoch.saturating_sub(epoch) > chain_finality {
            return Err(GossipBlockRejectReason::EpochBeyondFinality(
                epoch,
                heaviest_epoch,
            ));
        }
        Ok(())
    }

    /// Verify that block timestamp is consistent with its epoch:
    /// `timestamp == genesis_timestamp + epoch * block_delay`
    fn validate_timestamp(
        &self,
        genesis_tipset: &Tipset,
        block_delay: u32,
    ) -> Result<(), GossipBlockRejectReason> {
        let epoch = self.block.header.epoch;
        let timestamp = self.block.header.timestamp;
        // epoch is validated non-negative by validate_epoch_range before this
        let expected =
            genesis_tipset.min_timestamp() + (epoch as u64).saturating_mul(u64::from(block_delay));
        if timestamp != expected {
            return Err(GossipBlockRejectReason::TimestampMismatch {
                timestamp,
                epoch,
                expected,
            });
        }
        Ok(())
    }

    fn validate_election_proof(&self) -> Result<(), GossipBlockRejectReason> {
        match &self.block.header.election_proof {
            None => Err(GossipBlockRejectReason::MissingElectionProof),
            Some(proof) if proof.win_count < 1 => {
                Err(GossipBlockRejectReason::InvalidWinCount(proof.win_count))
            }
            _ => Ok(()),
        }
    }

    fn validate_signature_present(&self) -> Result<(), GossipBlockRejectReason> {
        if self.block.header.signature.is_none() {
            return Err(GossipBlockRejectReason::MissingSignature);
        }
        Ok(())
    }

    fn validate_message_count(&self) -> Result<(), GossipBlockRejectReason> {
        let count = self.block.bls_messages.len() + self.block.secpk_messages.len();
        if count > BLOCK_MESSAGE_LIMIT {
            return Err(GossipBlockRejectReason::TooManyMessages(count));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use crate::blocks::{CachingBlockHeader, ElectionProof, GossipBlock, RawBlockHeader, Tipset};
    use crate::chain_sync::bad_block_cache::{BadBlockCache, SeenBlockCache};
    use crate::db::MemoryDB;
    use crate::message::SignedMessage;
    use crate::shim::crypto::{Signature, SignatureType};
    use crate::shim::message::Message;
    use crate::test_utils::construct_messages;
    use crate::utils::encoding::from_slice_with_fallback;
    use base64::{Engine, prelude::BASE64_STANDARD};
    use cid::Cid;

    use super::{GossipBlockRejectReason, GossipBlockValidator, TipsetValidator};

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

    #[test]
    fn max_allowed_epoch_basic() {
        // genesis at t=1000, now at t=1300, block_delay=30
        // elapsed=300, 300/30=10, +5 drift = 15
        assert_eq!(super::max_allowed_epoch(1300, 1000, 30), Some(15));
    }

    #[test]
    fn max_allowed_epoch_at_genesis() {
        // now == genesis → epoch 0 + drift
        assert_eq!(super::max_allowed_epoch(1000, 1000, 30), Some(5));
    }

    #[test]
    fn max_allowed_epoch_clock_before_genesis() {
        // clock is behind genesis — should not panic, returns None
        assert_eq!(super::max_allowed_epoch(500, 1000, 30), None);
    }

    #[test]
    fn max_allowed_epoch_zero_block_delay() {
        // zero block delay would divide by zero — returns None
        assert_eq!(super::max_allowed_epoch(2000, 1000, 0), None);
    }

    fn make_gossip_block_with(f: impl FnOnce(&mut RawBlockHeader)) -> GossipBlock {
        let mut raw = RawBlockHeader {
            election_proof: Some(ElectionProof {
                win_count: 1,
                vrfproof: Default::default(),
            }),
            signature: Some(Signature {
                sig_type: SignatureType::Bls,
                bytes: vec![0u8; 96],
            }),
            ..Default::default()
        };
        f(&mut raw);
        GossipBlock {
            header: CachingBlockHeader::from(raw),
            bls_messages: vec![],
            secpk_messages: vec![],
        }
    }

    fn make_valid_gossip_block() -> GossipBlock {
        make_gossip_block_with(|_| {})
    }

    fn make_genesis() -> Tipset {
        Tipset::from(CachingBlockHeader::default())
    }

    #[test]
    fn gossip_block_validator_accepts_valid_block() {
        let block = make_valid_gossip_block();
        let genesis = make_genesis();
        let seen = SeenBlockCache::default();

        let result = GossipBlockValidator::new(&block).validate_pre_fetch(
            &genesis, 30,   // block_delay
            900,  // chain_finality
            0,    // heaviest_epoch (same as block epoch)
            None, // no bad block cache
            &seen,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn gossip_block_validator_rejects_duplicate() {
        let block = make_valid_gossip_block();
        let genesis = make_genesis();
        let seen = SeenBlockCache::default();

        assert!(
            GossipBlockValidator::new(&block)
                .validate_pre_fetch(&genesis, 30, 900, 0, None, &seen)
                .is_ok()
        );

        let err = GossipBlockValidator::new(&block)
            .validate_pre_fetch(&genesis, 30, 900, 0, None, &seen)
            .unwrap_err();
        assert!(matches!(err, GossipBlockRejectReason::DuplicateBlock(_)));
    }

    #[test]
    fn gossip_block_validator_rejects_bad_block() {
        let block = make_valid_gossip_block();
        let genesis = make_genesis();
        let seen = SeenBlockCache::default();
        let bad_cache = BadBlockCache::default();
        bad_cache.push(*block.header.cid());

        let err = GossipBlockValidator::new(&block)
            .validate_pre_fetch(&genesis, 30, 900, 0, Some(&bad_cache), &seen)
            .unwrap_err();
        assert!(matches!(err, GossipBlockRejectReason::BadBlock(_)));
    }

    #[test]
    fn gossip_block_validator_rejects_epoch_too_far_ahead() {
        let block = make_gossip_block_with(|h| h.epoch = i64::MAX);
        let genesis = make_genesis();
        let seen = SeenBlockCache::default();

        let err = GossipBlockValidator::new(&block)
            .validate_pre_fetch(&genesis, 30, 900, 0, None, &seen)
            .unwrap_err();
        assert!(matches!(err, GossipBlockRejectReason::EpochTooFarAhead(_)));
    }

    #[test]
    fn gossip_block_validator_rejects_epoch_beyond_finality() {
        let block = make_valid_gossip_block(); // epoch = 0
        let genesis = make_genesis();
        let seen = SeenBlockCache::default();

        let err = GossipBlockValidator::new(&block)
            .validate_pre_fetch(&genesis, 30, 900, 1000, None, &seen)
            .unwrap_err();
        assert!(matches!(
            err,
            GossipBlockRejectReason::EpochBeyondFinality(_, _)
        ));
    }

    #[test]
    fn gossip_block_validator_rejects_missing_election_proof() {
        let block = make_gossip_block_with(|h| h.election_proof = None);
        let genesis = make_genesis();
        let seen = SeenBlockCache::default();

        let err = GossipBlockValidator::new(&block)
            .validate_pre_fetch(&genesis, 30, 900, 0, None, &seen)
            .unwrap_err();
        assert!(matches!(err, GossipBlockRejectReason::MissingElectionProof));
    }

    #[test]
    fn gossip_block_validator_rejects_zero_win_count() {
        let block = make_gossip_block_with(|h| {
            h.election_proof = Some(ElectionProof {
                win_count: 0,
                vrfproof: Default::default(),
            })
        });
        let genesis = make_genesis();
        let seen = SeenBlockCache::default();

        let err = GossipBlockValidator::new(&block)
            .validate_pre_fetch(&genesis, 30, 900, 0, None, &seen)
            .unwrap_err();
        assert!(matches!(err, GossipBlockRejectReason::InvalidWinCount(0)));
    }

    #[test]
    fn gossip_block_validator_rejects_missing_signature() {
        let block = make_gossip_block_with(|h| h.signature = None);
        let genesis = make_genesis();
        let seen = SeenBlockCache::default();

        let err = GossipBlockValidator::new(&block)
            .validate_pre_fetch(&genesis, 30, 900, 0, None, &seen)
            .unwrap_err();
        assert!(matches!(err, GossipBlockRejectReason::MissingSignature));
    }

    #[test]
    fn gossip_block_validator_rejects_too_many_messages() {
        let mut block = make_valid_gossip_block();
        block.bls_messages = vec![Cid::default(); 10_001];
        let genesis = make_genesis();
        let seen = SeenBlockCache::default();

        let err = GossipBlockValidator::new(&block)
            .validate_pre_fetch(&genesis, 30, 900, 0, None, &seen)
            .unwrap_err();
        assert!(matches!(err, GossipBlockRejectReason::TooManyMessages(_)));
    }

    #[test]
    fn gossip_block_validator_rejects_negative_epoch() {
        let block = make_gossip_block_with(|h| h.epoch = -1);
        let genesis = make_genesis();
        let seen = SeenBlockCache::default();

        let err = GossipBlockValidator::new(&block)
            .validate_pre_fetch(&genesis, 30, 900, 0, None, &seen)
            .unwrap_err();
        assert!(matches!(err, GossipBlockRejectReason::NegativeEpoch(-1)));
    }

    #[test]
    fn gossip_block_validator_rejects_timestamp_mismatch() {
        // epoch=0, genesis timestamp=0, so expected timestamp = 0 + 0*30 = 0
        // but we set timestamp=999
        let block = make_gossip_block_with(|h| h.timestamp = 999);
        let genesis = make_genesis();
        let seen = SeenBlockCache::default();

        let err = GossipBlockValidator::new(&block)
            .validate_pre_fetch(&genesis, 30, 900, 0, None, &seen)
            .unwrap_err();
        assert!(matches!(
            err,
            GossipBlockRejectReason::TimestampMismatch { .. }
        ));
    }

    #[test]
    fn rejected_block_not_cached_as_seen() {
        // A block rejected for a transient reason (e.g., epoch too far ahead)
        // must NOT be inserted into the seen cache. Otherwise, if the same
        // block is received later when it becomes valid, it would be
        // incorrectly suppressed as a duplicate.
        let block = make_gossip_block_with(|h| h.epoch = i64::MAX);
        let genesis = make_genesis();
        let seen = SeenBlockCache::default();

        // First attempt: rejected as too far ahead
        let err = GossipBlockValidator::new(&block)
            .validate_pre_fetch(&genesis, 30, 900, 0, None, &seen)
            .unwrap_err();
        assert!(matches!(err, GossipBlockRejectReason::EpochTooFarAhead(_)));

        // Second attempt: must still be EpochTooFarAhead, NOT DuplicateBlock
        let err = GossipBlockValidator::new(&block)
            .validate_pre_fetch(&genesis, 30, 900, 0, None, &seen)
            .unwrap_err();
        assert!(matches!(err, GossipBlockRejectReason::EpochTooFarAhead(_)));
    }

    #[test]
    fn seen_block_cache_deduplicates() {
        let cache = SeenBlockCache::default();
        let cid = Cid::default();

        assert!(!cache.test_and_insert(&cid));
        assert!(cache.test_and_insert(&cid));
    }
}

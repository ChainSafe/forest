// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::{CachingBlockHeader, TipsetKey};
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use cid::Cid;

fn create_test_header(
    miner: Address,
    epoch: ChainEpoch,
    parents: TipsetKey,
    timestamp: Option<u64>,
) -> CachingBlockHeader {
    use crate::blocks::RawBlockHeader;
    use crate::shim::econ::TokenAmount;
    use num::BigInt;

    let raw_header = RawBlockHeader {
        miner_address: miner,
        epoch,
        parents,
        weight: BigInt::from(0),
        state_root: Cid::default(),
        message_receipts: Cid::default(),
        messages: Cid::default(),
        timestamp: timestamp.unwrap_or(0),
        parent_base_fee: TokenAmount::from_atto(0),
        ticket: None,
        election_proof: None,
        beacon_entries: Vec::new(),
        winning_post_proof: Vec::new(),
        bls_aggregate: None,
        signature: None,
        fork_signal: 0,
    };

    CachingBlockHeader::new(raw_header)
}

#[test]
fn test_filter_double_fork_mining() {
    use crate::slasher::filter::SlasherFilter;
    use crate::slasher::types::ConsensusFaultType;

    let _ = std::fs::remove_dir_all(std::env::temp_dir().join("slasher_test"));
    let mut filter = SlasherFilter::new(std::env::temp_dir().join("slasher_test"))
        .expect("Failed to create slasher filter");

    let miner = Address::new_id(1000);
    let epoch = 100;
    let parents = TipsetKey::from(nunny::vec![Cid::default()]);

    // Process first block - should not detect any fault
    let header1 = create_test_header(miner, epoch, parents.clone(), Some(1000));
    let result1 = filter
        .process_block(&header1)
        .expect("Failed to process first block");
    assert!(result1.is_none());

    // Process second block - should detect double-fork mining
    let header2 = create_test_header(miner, epoch, parents, Some(2000));
    let result2 = filter
        .process_block(&header2)
        .expect("Failed to process second block");
    assert!(result2.is_some());

    if let Some(fault) = result2 {
        assert_eq!(fault.fault_type, ConsensusFaultType::DoubleForkMining);
        assert_eq!(fault.miner_address, miner);
        assert_eq!(fault.detection_epoch, epoch);
        assert_eq!(fault.block_headers.len(), 2);
        assert!(fault.extra_evidence.is_none());
    }
}

#[test]
fn test_filter_time_offset_mining() {
    use crate::slasher::filter::SlasherFilter;
    use crate::slasher::types::ConsensusFaultType;

    let _ = std::fs::remove_dir_all(std::env::temp_dir().join("slasher_test"));
    let mut filter = SlasherFilter::new(std::env::temp_dir().join("slasher_test"))
        .expect("Failed to create slasher filter");

    let miner = Address::new_id(1000);
    let epoch = 100;
    let parents = TipsetKey::from(nunny::vec![Cid::default()]);

    // Process first block with specific parents
    let header1 = create_test_header(miner, epoch, parents.clone(), Some(1000));
    let result1 = filter
        .process_block(&header1)
        .expect("Failed to process first block");
    assert!(result1.is_none());

    // Process second block with same parents but different timestamp - should detect time-offset mining
    let header2 = create_test_header(miner, epoch, parents, Some(2000));
    let result2 = filter
        .process_block(&header2)
        .expect("Failed to process second block");
    assert!(result2.is_some());

    if let Some(fault) = result2 {
        // Note: With same parents, double-fork mining is detected first
        assert_eq!(fault.fault_type, ConsensusFaultType::DoubleForkMining);
        assert_eq!(fault.miner_address, miner);
        assert_eq!(fault.detection_epoch, epoch);
        assert_eq!(fault.block_headers.len(), 2);
        assert!(fault.extra_evidence.is_none());
    }
}

#[test]
fn test_filter_parent_grinding() {
    use crate::slasher::filter::SlasherFilter;
    use crate::slasher::types::ConsensusFaultType;

    let _ = std::fs::remove_dir_all(std::env::temp_dir().join("slasher_test"));
    let mut filter = SlasherFilter::new(std::env::temp_dir().join("slasher_test"))
        .expect("Failed to create slasher filter");

    let miner = Address::new_id(1000);
    let epoch1 = 100;
    let epoch2 = 101;
    let parents1 = TipsetKey::from(nunny::vec![Cid::default()]);

    // Create miner's block at an epoch
    let header1 = create_test_header(miner, epoch1, parents1.clone(), Some(1000));
    let result1 = filter
        .process_block(&header1)
        .expect("Failed to process first block");
    assert!(result1.is_none());

    // Create another block at same epoch but different parents
    let parents2 = TipsetKey::from(nunny::vec![Cid::default()]);
    let header2 = create_test_header(Address::new_id(2000), epoch1, parents2.clone(), Some(2000));
    let result2 = filter
        .process_block(&header2)
        .expect("Failed to process second block");
    assert!(result2.is_none());

    let header3 = create_test_header(
        miner,
        epoch2,
        TipsetKey::from(nunny::vec![*header2.cid()]),
        Some(3000),
    );
    let result3 = filter
        .process_block(&header3)
        .expect("Failed to process third block");
    assert!(result3.is_some());

    if let Some(fault) = result3 {
        assert_eq!(fault.fault_type, ConsensusFaultType::ParentGrinding);
        assert_eq!(fault.miner_address, miner);
        assert_eq!(fault.detection_epoch, epoch2);
        assert_eq!(fault.block_headers.len(), 2);
        assert!(fault.extra_evidence.is_some());
    }
}

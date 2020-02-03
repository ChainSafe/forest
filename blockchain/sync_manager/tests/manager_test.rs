// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blocks::{BlockHeader, Tipset};
use cid::Cid;
use num_bigint::BigUint;
use sync_manager::SyncManager;

fn create_header(weight: u64, parent_bz: &[u8], cached_bytes: &[u8]) -> BlockHeader {
    let header = BlockHeader::builder()
        .weight(BigUint::from(weight))
        .cached_bytes(cached_bytes.to_vec())
        .cached_cid(Cid::from_bytes_default(parent_bz).unwrap())
        .build()
        .unwrap();
    header
}

#[test]
fn schedule_tipset() {
    let header = create_header(0, b"", b"");
    let tipset = Tipset::new(vec![header]).unwrap();
    let mut manager = SyncManager::default();
    manager.schedule_tipset(&tipset);
    {
        // Test scheduling inside different scope
        manager.schedule_tipset(&tipset);
    }
    manager.schedule_tipset(&tipset);
}

#[test]
fn heaviest_different_chain() {
    let l_tipset = Tipset::new(vec![create_header(1, b"1", b"1")]).unwrap();
    let m_tipset = Tipset::new(vec![create_header(2, b"2", b"2")]).unwrap();
    let h_tipset = Tipset::new(vec![create_header(3, b"1", b"1")]).unwrap();
    let mut manager = SyncManager::default();
    manager.schedule_tipset(&l_tipset);
    manager.schedule_tipset(&m_tipset);
    manager.schedule_tipset(&h_tipset);
    assert_eq!(manager.select_sync_target().unwrap(), &h_tipset);
    assert_ne!(manager.select_sync_target().unwrap(), &l_tipset);
}

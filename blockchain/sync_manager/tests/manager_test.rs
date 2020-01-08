use address::Address;
use blocks::{BlockHeader, TipSetKeys, Tipset};
use cid::{Cid, Codec, Version};
use sync_manager::SyncManager;

fn create_header(weight: u64, parent_bz: &[u8], cached_bytes: &[u8]) -> BlockHeader {
    let x = TipSetKeys {
        cids: vec![Cid::new(Codec::DagCBOR, Version::V1, parent_bz)],
    };
    BlockHeader::builder()
        .parents(x)
        .cached_bytes(cached_bytes.to_vec()) // TODO change to however cached bytes are generated in future
        .miner_address(Address::new_id(0).unwrap())
        .bls_aggregate(vec![])
        .weight(weight)
        .build()
        .unwrap()
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

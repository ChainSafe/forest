use address::Address;
use blocks::{BlockHeader, TipSetKeys, Tipset};
use sync_manager::SyncManager;

#[test]
fn schedule_tipset() {
    let header = BlockHeader::builder()
        .parents(TipSetKeys::default())
        .miner_address(Address::new_id(0).unwrap())
        .bls_aggregate(vec![])
        .weight(0)
        .build()
        .unwrap();

    let tipset = Tipset::new(vec![header.clone()]).unwrap();
    let mut manager = SyncManager::default();
    manager.schedule_tipset(&tipset);
    {
        // Test scheduling inside different scope
        manager.schedule_tipset(&tipset);
    }

    manager.schedule_tipset(&tipset);
}

use super::*;
use async_std::sync::channel;
use db::MemoryDB;

#[test]
fn peer_manager_update() {
    let db = MemoryDB::default();
    let (local_sender, _test_receiver) = channel(20);
    let (_event_sender, event_receiver) = channel(20);

    let cs = ChainSyncer::new(Arc::new(db), local_sender, event_receiver).unwrap();
    let peer_manager = Arc::clone(&cs.peer_manager);
}

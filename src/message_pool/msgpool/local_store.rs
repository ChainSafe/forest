// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Tracks local-wallet senders and the messages they have published.
//!
//! Only messages from these senders are eligible for republishing, and only
//! these messages are replayed into the pending store on `load_local`.

use ahash::HashSet;
use parking_lot::RwLock as SyncRwLock;

use crate::message::SignedMessage;
use crate::shim::address::Address;

#[derive(Default)]
pub(in crate::message_pool) struct LocalStore {
    addrs: SyncRwLock<HashSet<Address>>,
    msgs: SyncRwLock<HashSet<SignedMessage>>,
}

impl LocalStore {
    pub(in crate::message_pool) fn new() -> Self {
        Self::default()
    }

    pub(in crate::message_pool) fn add(&self, msg: SignedMessage, resolved_from: Address) {
        self.addrs.write().insert(resolved_from);
        self.msgs.write().insert(msg);
    }

    pub(in crate::message_pool) fn known_local_addrs(&self) -> HashSet<Address> {
        self.addrs.read().clone()
    }

    pub(in crate::message_pool) fn snapshot_msgs(&self) -> Vec<SignedMessage> {
        self.msgs.read().iter().cloned().collect()
    }

    pub(in crate::message_pool) fn remove_msg(&self, msg: &SignedMessage) {
        self.msgs.write().remove(msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::MessageRead as _;
    use crate::shim::econ::TokenAmount;
    use crate::shim::message::Message as ShimMessage;

    fn make_smsg(from: Address, seq: u64) -> SignedMessage {
        SignedMessage::mock_bls_signed_message(ShimMessage {
            from,
            sequence: seq,
            gas_premium: TokenAmount::from_atto(100u64),
            gas_limit: 1_000_000,
            ..ShimMessage::default()
        })
    }

    #[test]
    fn add_records_address_and_message() {
        let store = LocalStore::new();
        let addr = Address::new_id(1);
        let msg = make_smsg(addr, 0);

        store.add(msg.clone(), addr);

        assert_eq!(store.known_local_addrs(), vec![addr]);
        let msgs = store.snapshot_msgs();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].sequence(), 0);
    }

    #[test]
    fn add_appends_addresses_in_order() {
        let store = LocalStore::new();
        let a1 = Address::new_id(1);
        let a2 = Address::new_id(2);

        store.add(make_smsg(a1, 0), a1);
        store.add(make_smsg(a2, 0), a2);

        assert_eq!(store.known_local_addrs(), vec![a1, a2]);
    }

    #[test]
    fn remove_msg_drops_only_the_named_message() {
        let store = LocalStore::new();
        let addr = Address::new_id(1);
        let m0 = make_smsg(addr, 0);
        let m1 = make_smsg(addr, 1);

        store.add(m0.clone(), addr);
        store.add(m1.clone(), addr);
        store.remove_msg(&m0);

        let remaining = store.snapshot_msgs();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].sequence(), 1);
    }
}

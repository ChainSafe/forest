// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::db::{SettingsStore, SettingsStoreExt as _};
use crate::eth::EthChainId;
use crate::key_management::{Key, sign_message};
use crate::message_pool::MessagePool;
use crate::message_pool::msgpool::provider::Provider;
use crate::shim::address::Address;
use crate::shim::message::Message;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, warn};

/// Serializes nonce assignment globally and persists the next expected nonce
/// per address. The global mutex prevents concurrent nonce assignment across
/// all senders, while persistence ensures in-flight nonces survive restarts.
///
/// Nonces are persisted at `/mpool/nonces/{addr}` in the [`SettingsStore`]. On
/// restart, [`next_nonce`](Self::next_nonce) returns
/// `max(mpool_nonce, persisted_nonce)` so that in-flight nonces are not reused.
/// Nonces are only persisted **after** a successful push; if signing or pushing
/// fails the nonce is not consumed.
///
/// See also [`MpoolLocker`](super::MpoolLocker), the outer per-sender lock.
pub struct NonceTracker {
    lock: Mutex<()>,
    store: Arc<dyn SettingsStore + Send + Sync>,
}

impl NonceTracker {
    pub fn new(store: Arc<dyn SettingsStore + Send + Sync>) -> Self {
        Self {
            lock: Mutex::new(()),
            store,
        }
    }

    fn nonce_key(addr: &Address) -> String {
        format!("/mpool/nonces/{addr}")
    }

    /// Return `max(mpool_nonce, persisted_nonce)`.
    /// Warns if the `mpool` nonce exceeds the persisted nonce.
    pub fn next_nonce<T: Provider>(
        &self,
        mpool: &MessagePool<T>,
        addr: &Address,
    ) -> anyhow::Result<u64> {
        let mpool_nonce = mpool.get_sequence(addr)?;

        let key = Self::nonce_key(addr);
        match self.store.read_obj::<u64>(&key)? {
            None => Ok(mpool_nonce),
            Some(ds_nonce) => {
                if mpool_nonce <= ds_nonce {
                    Ok(ds_nonce)
                } else {
                    warn!(
                        "mempool nonce was larger than datastore nonce ({} > {})",
                        mpool_nonce, ds_nonce
                    );
                    Ok(mpool_nonce)
                }
            }
        }
    }

    /// Persist `nonce + 1` for the given address.
    pub fn save_nonce(&self, addr: &Address, nonce: u64) -> anyhow::Result<()> {
        let key = Self::nonce_key(addr);
        self.store.write_obj(&key, &(nonce + 1))
    }

    /// Acquire the global lock, assign a nonce, sign, push to `mpool`, and
    /// persist the nonce. If the push fails the nonce is NOT persisted.
    pub async fn sign_and_push<T: Provider + Send + Sync>(
        &self,
        mpool: &MessagePool<T>,
        mut message: Message,
        key: &Key,
        eth_chain_id: EthChainId,
    ) -> anyhow::Result<crate::message::SignedMessage> {
        let _guard = self.lock.lock().await;

        let nonce = self.next_nonce(mpool, &message.from)?;
        message.sequence = nonce;

        let smsg = sign_message(key, &message, eth_chain_id)?;

        mpool.push(smsg.clone()).await?;

        if let Err(err) = self.save_nonce(&message.from, nonce) {
            error!(
                from = %message.from,
                nonce,
                "message pushed but failed to persist next nonce: {err}"
            );
        }

        Ok(smsg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::MemoryDB;
    use crate::key_management::{KeyStore, KeyStoreConfig, Wallet};
    use crate::message_pool::MessagePool;
    use crate::message_pool::msgpool::test_provider::TestApi;
    use crate::shim::crypto::SignatureType;
    use crate::shim::{address::Address, econ::TokenAmount};
    use std::sync::Arc;
    use tokio::task::JoinSet;

    fn make_test_nonce_store() -> Arc<MemoryDB> {
        Arc::new(MemoryDB::default())
    }

    fn make_test_pool_and_wallet() -> (
        MessagePool<TestApi>,
        Wallet,
        Address,
        flume::Receiver<crate::libp2p::NetworkMessage>,
    ) {
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let sender = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let tma = TestApi::default();
        tma.set_state_sequence(&sender, 0);
        tma.set_state_balance_raw(&sender, TokenAmount::from_whole(1000));
        let (tx, rx) = flume::bounded(50);
        let mut services = JoinSet::new();
        let mpool = MessagePool::new(
            tma,
            tx,
            Default::default(),
            Default::default(),
            &mut services,
        )
        .unwrap();
        (mpool, wallet, sender, rx)
    }

    fn make_message(from: Address) -> Message {
        Message {
            from,
            to: Address::new_id(99),
            value: TokenAmount::from_atto(1),
            method_num: 0,
            sequence: 0,
            gas_limit: 10_000_000,
            gas_fee_cap: TokenAmount::from_atto(10_000),
            gas_premium: TokenAmount::from_atto(100),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_next_nonce_uses_mpool_when_no_persisted() {
        let store = make_test_nonce_store();
        let tracker = NonceTracker::new(store);
        let (mpool, _wallet, sender, _rx) = make_test_pool_and_wallet();

        let nonce = tracker.next_nonce(&mpool, &sender).unwrap();
        assert_eq!(
            nonce, 0,
            "should return mpool nonce when nothing is persisted"
        );
    }

    #[tokio::test]
    async fn test_next_nonce_uses_max_of_mpool_and_persisted() {
        let store = make_test_nonce_store();
        let tracker = NonceTracker::new(store);
        let (mpool, _wallet, sender, _rx) = make_test_pool_and_wallet();

        tracker.save_nonce(&sender, 9).unwrap();

        let nonce = tracker.next_nonce(&mpool, &sender).unwrap();
        assert_eq!(
            nonce, 10,
            "should return persisted nonce (9+1=10) when > mpool nonce (0)"
        );
    }

    #[tokio::test]
    async fn test_save_nonce_persists_incremented() {
        let store = make_test_nonce_store();
        let tracker = NonceTracker::new(store.clone());

        let addr = Address::new_id(42);
        tracker.save_nonce(&addr, 5).unwrap();

        let key = NonceTracker::nonce_key(&addr);
        let stored: u64 = store.read_obj(&key).unwrap().unwrap();
        assert_eq!(stored, 6, "save_nonce(5) should persist 6");
    }

    #[tokio::test]
    async fn test_sign_and_push_assigns_sequential_nonces() {
        let store = make_test_nonce_store();
        let tracker = NonceTracker::new(store);
        let (mpool, mut wallet, sender, _rx) = make_test_pool_and_wallet();

        let key = wallet.find_key(&sender).unwrap();
        let eth_chain_id = 0u64;

        let msg1 = make_message(sender);
        let smsg1 = tracker
            .sign_and_push(&mpool, msg1, &key, eth_chain_id)
            .await
            .unwrap();
        assert_eq!(smsg1.message().sequence, 0);

        let msg2 = make_message(sender);
        let smsg2 = tracker
            .sign_and_push(&mpool, msg2, &key, eth_chain_id)
            .await
            .unwrap();
        assert_eq!(smsg2.message().sequence, 1);
    }

    #[tokio::test]
    async fn test_sign_and_push_does_not_persist_on_push_failure() {
        let store = make_test_nonce_store();
        let tracker = NonceTracker::new(store.clone());
        let (mpool, mut wallet, sender, _rx) = make_test_pool_and_wallet();

        let key = wallet.find_key(&sender).unwrap();
        let eth_chain_id = 0u64;

        let mut msg = make_message(sender);
        msg.gas_limit = 0;

        let result = tracker.sign_and_push(&mpool, msg, &key, eth_chain_id).await;
        assert!(result.is_err(), "push should fail with zero gas limit");

        let key_str = NonceTracker::nonce_key(&sender);
        let stored: Option<u64> = store.read_obj(&key_str).unwrap();
        assert!(
            stored.is_none(),
            "nonce should NOT be persisted when push fails"
        );
    }
}

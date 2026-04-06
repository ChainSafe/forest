// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::eth::EthChainId;
use crate::key_management::{Key, sign_message};
use crate::message_pool::MessagePool;
use crate::message_pool::msgpool::provider::Provider;
use crate::shim::message::Message;
use tokio::sync::Mutex;

/// Serializes nonce assignment globally. The global mutex prevents concurrent
/// nonce assignment across all senders, ensuring sequential nonce values.
///
/// See also [`MpoolLocker`](super::MpoolLocker), the outer per-sender lock.
pub struct NonceTracker {
    lock: Mutex<()>,
}

impl NonceTracker {
    pub fn new() -> Self {
        Self {
            lock: Mutex::new(()),
        }
    }

    /// Acquire the global lock, assign a nonce, sign, and push to `mpool`.
    pub async fn sign_and_push<T: Provider + Send + Sync>(
        &self,
        mpool: &MessagePool<T>,
        mut message: Message,
        key: &Key,
        eth_chain_id: EthChainId,
    ) -> anyhow::Result<crate::message::SignedMessage> {
        let _guard = self.lock.lock().await;

        let nonce = mpool.get_sequence(&message.from)?;
        message.sequence = nonce;

        let smsg = sign_message(key, &message, eth_chain_id)?;
        mpool.push(smsg.clone()).await?;
        Ok(smsg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_management::{KeyStore, KeyStoreConfig, Wallet};
    use crate::message_pool::MessagePool;
    use crate::message_pool::msgpool::test_provider::TestApi;
    use crate::shim::crypto::SignatureType;
    use crate::shim::{address::Address, econ::TokenAmount};
    use std::sync::Arc;
    use tokio::task::JoinSet;

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
    async fn test_sign_and_push_assigns_sequential_nonces() {
        let tracker = NonceTracker::new();
        let (mpool, mut wallet, sender, _rx) = make_test_pool_and_wallet();

        let key = wallet.find_key(&sender).unwrap();
        let eth_chain_id: EthChainId = crate::networks::calibnet::ETH_CHAIN_ID;

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
    async fn test_concurrent_push_no_nonce_duplicates() {
        const N: usize = 10;
        let tracker = Arc::new(NonceTracker::new());
        let (mpool, mut wallet, sender, _rx) = make_test_pool_and_wallet();
        let mpool = Arc::new(mpool);
        let key = Arc::new(wallet.find_key(&sender).unwrap());
        let eth_chain_id: EthChainId = crate::networks::calibnet::ETH_CHAIN_ID;

        let mut tasks = JoinSet::new();
        for _ in 0..N {
            let (tracker, mpool, key) = (tracker.clone(), mpool.clone(), key.clone());
            tasks.spawn(async move {
                tracker
                    .sign_and_push(&mpool, make_message(sender), &key, eth_chain_id)
                    .await
                    .unwrap()
                    .message()
                    .sequence
            });
        }

        let mut nonces: Vec<u64> = tasks.join_all().await;
        nonces.sort();

        let expected: Vec<u64> = (0..N as u64).collect();
        assert_eq!(nonces, expected, "nonces must be contiguous 0..{N}");
    }
}

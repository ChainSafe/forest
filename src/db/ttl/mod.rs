// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::eth::EthChainId;
use crate::message::ChainMessage;
use crate::rpc::eth::{eth_tx_from_signed_eth_message, types::EthHash};
use fvm_ipld_blockstore::Blockstore;
use std::sync::Arc;
use std::time::Duration;

use super::EthMappingsStore;

pub struct EthMappingCollector<DB> {
    db: Arc<DB>,
    eth_chain_id: EthChainId,
    ttl: std::time::Duration,
}

impl<DB: Blockstore + EthMappingsStore + Sync + Send + 'static> EthMappingCollector<DB> {
    /// Creates a `TTL` collector for the Ethereum mapping.
    ///
    pub fn new(db: Arc<DB>, eth_chain_id: EthChainId, ttl: Duration) -> Self {
        Self {
            db,
            eth_chain_id,
            ttl,
        }
    }

    /// Remove keys whose `(duration - timestamp) > TTL` from the database
    /// where `duration` is the elapsed time since "UNIX timestamp".
    fn ttl_workflow(&self, duration: Duration) -> anyhow::Result<()> {
        let keys: Vec<EthHash> = self
            .db
            .get_message_cids()?
            .iter()
            .filter(|(_, timestamp)| {
                duration.saturating_sub(Duration::from_secs(*timestamp)) > self.ttl
            })
            .filter_map(|(cid, _)| {
                let message = crate::chain::get_chain_message(self.db.as_ref(), cid);
                if let Ok(ChainMessage::Signed(smsg)) = message {
                    let result = eth_tx_from_signed_eth_message(&smsg, self.eth_chain_id);
                    if let Ok((_, tx)) = result {
                        tx.eth_hash().ok().map(EthHash)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        for h in keys.iter() {
            tracing::trace!("Marked {} for deletion", h);
        }
        let count = keys.len();
        self.db.delete(keys)?;

        tracing::debug!(
            "Found and deleted {count} mappings older than {:?}",
            self.ttl
        );

        Ok(())
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        loop {
            tokio::time::sleep(self.ttl).await;

            let duration = Duration::from_secs(chrono::Utc::now().timestamp() as u64);
            self.ttl_workflow(duration)?;
        }
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use chrono::{DateTime, TimeZone, Utc};
    use cid::Cid;

    use crate::chain_sync::TipsetValidator;
    use crate::db::EthMappingsStore;
    use crate::db::EthMappingsStoreExt;
    use crate::db::MemoryDB;
    use crate::networks::calibnet::ETH_CHAIN_ID;
    use crate::test_utils::construct_eth_messages;

    const ZERO_DURATION: Duration = Duration::from_secs(0);
    const EPS_DURATION: Duration = Duration::from_secs(1);
    const TTL_DURATION: Duration = Duration::from_secs(60);

    use super::*;

    #[tokio::test]
    async fn test_ttl() {
        let blockstore = Arc::new(MemoryDB::default());

        let (bls0, secp0) = construct_eth_messages(0);
        let (bls1, secp1) = construct_eth_messages(1);

        crate::chain::persist_objects(&blockstore, [bls0.clone(), bls1.clone()].iter()).unwrap();
        crate::chain::persist_objects(&blockstore, [secp0.clone(), secp1.clone()].iter()).unwrap();

        let expected_root =
            Cid::try_from("bafy2bzacebqzqoow32yddtu746myprecdtblty77f3k6at6v2axkhvqd3iwvi")
                .unwrap();

        let root = TipsetValidator::compute_msg_root(
            &blockstore,
            &[bls0.clone(), bls1.clone()],
            &[secp0.clone(), secp1.clone()],
        )
        .expect("Computing message root should succeed");
        assert_eq!(root, expected_root);

        // Unix epoch corresponds to 1970-01-01 00:00:00 UTC
        let unix_timestamp: DateTime<Utc> = Utc.timestamp_opt(0, 0).unwrap();

        // Add key0 with unix epoch
        let (_, tx0) = eth_tx_from_signed_eth_message(&secp0, ETH_CHAIN_ID).unwrap();
        let key0 = tx0.eth_hash().unwrap().into();

        let timestamp = unix_timestamp.timestamp() as u64;
        blockstore
            .write_obj(&key0, &(secp0.cid(), timestamp))
            .unwrap();

        assert!(blockstore.exists(&key0).unwrap());

        // Add key1 with unix epoch + 2 * ttl
        let (_, tx1) = eth_tx_from_signed_eth_message(&secp1, ETH_CHAIN_ID).unwrap();
        let key1 = tx1.eth_hash().unwrap().into();

        blockstore
            .write_obj(
                &key1,
                &(
                    secp1.cid(),
                    unix_timestamp.timestamp() as u64 + 2 * TTL_DURATION.as_secs(),
                ),
            )
            .unwrap();

        assert!(blockstore.exists(&key1).unwrap());

        let collector = EthMappingCollector::new(blockstore.clone(), ETH_CHAIN_ID, TTL_DURATION);

        collector.ttl_workflow(ZERO_DURATION).unwrap();

        assert!(blockstore.exists(&key0).unwrap());
        assert!(blockstore.exists(&key1).unwrap());

        collector.ttl_workflow(TTL_DURATION + EPS_DURATION).unwrap();

        assert!(!blockstore.exists(&key0).unwrap());
        assert!(blockstore.exists(&key1).unwrap());
    }
}

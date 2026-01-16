// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::{Tipset, TipsetKey};
use crate::chain::{ChainStore, Error as ChainError};
use ahash::{HashMap, HashMapExt};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;

use super::{
    ChainExchangeRequest, ChainExchangeResponse, ChainExchangeResponseStatus, CompactedMessages,
    TipsetBundle,
};

/// Builds chain exchange response out of chain data.
pub fn make_chain_exchange_response<DB>(
    cs: &ChainStore<DB>,
    request: &ChainExchangeRequest,
) -> ChainExchangeResponse
where
    DB: Blockstore + Send + Sync + 'static,
{
    if !request.is_options_valid() {
        return ChainExchangeResponse {
            chain: Default::default(),
            status: ChainExchangeResponseStatus::BadRequest,
            message: format!("Invalid options {}", request.options),
        };
    }

    let inner = move || {
        let root = match cs
            .chain_index()
            .load_tipset(&TipsetKey::from(request.start.clone()))?
        {
            Some(tipset) => tipset,
            None => {
                return Ok(ChainExchangeResponse {
                    status: ChainExchangeResponseStatus::BlockNotFound,
                    chain: Default::default(),
                    message: "Start tipset was not found in the database".into(),
                });
            }
        };

        let chain: Vec<_> = cs
            .chain_index()
            .chain(root)
            .take(request.request_len as _)
            .map(|tipset| {
                let mut tipset_bundle: TipsetBundle = TipsetBundle::default();
                if request.include_messages() {
                    tipset_bundle.messages = Some(compact_messages(cs.blockstore(), &tipset)?);
                }

                if request.include_blocks() {
                    tipset_bundle.blocks = tipset.block_headers().iter().cloned().collect_vec();
                }

                anyhow::Ok(tipset_bundle)
            })
            .try_collect()?;

        anyhow::Ok(ChainExchangeResponse {
            status: if request.request_len > chain.len() as u64 {
                ChainExchangeResponseStatus::PartialResponse
            } else {
                ChainExchangeResponseStatus::Success
            },
            chain,
            message: "Success".into(),
        })
    };

    match inner() {
        Ok(r) => r,
        Err(e) => ChainExchangeResponse {
            chain: Default::default(),
            status: ChainExchangeResponseStatus::InternalError,
            message: e.to_string(),
        },
    }
}

// Builds CompactedMessages for given Tipset.
fn compact_messages<DB>(db: &DB, tipset: &Tipset) -> Result<CompactedMessages, ChainError>
where
    DB: Blockstore,
{
    let mut bls_messages_order = HashMap::new();
    let mut secp_messages_order = HashMap::new();
    let mut bls_cids_combined: Vec<Cid> = vec![];
    let mut secp_cids_combined: Vec<Cid> = vec![];
    let mut bls_msg_includes: Vec<Vec<u64>> = vec![];
    let mut secp_msg_includes: Vec<Vec<u64>> = vec![];

    for block_header in tipset.block_headers().iter() {
        let (bls_cids, secp_cids) = crate::chain::read_msg_cids(db, block_header)?;

        let mut bls_include = Vec::with_capacity(bls_cids.len());
        for bls_cid in bls_cids.into_iter() {
            let order = match bls_messages_order.get(&bls_cid) {
                Some(order) => *order,
                None => {
                    let order = bls_cids_combined.len() as u64;
                    bls_cids_combined.push(bls_cid);
                    bls_messages_order.insert(bls_cid, order);
                    order
                }
            };

            bls_include.push(order);
        }

        bls_msg_includes.push(bls_include);

        let mut secp_include = Vec::with_capacity(secp_cids.len());
        for secp_cid in secp_cids.into_iter() {
            let order = match secp_messages_order.get(&secp_cid) {
                Some(order) => *order,
                None => {
                    let order = secp_cids_combined.len() as u64;
                    secp_cids_combined.push(secp_cid);
                    secp_messages_order.insert(secp_cid, order);
                    order
                }
            };

            secp_include.push(order);
        }
        secp_msg_includes.push(secp_include);
    }

    let (bls_msgs, secp_msgs) =
        crate::chain::block_messages_from_cids(db, &bls_cids_combined, &secp_cids_combined)?;

    Ok(CompactedMessages {
        bls_msgs,
        bls_msg_includes,
        secp_msgs,
        secp_msg_includes,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        super::{HEADERS, MESSAGES},
        *,
    };
    use crate::blocks::{CachingBlockHeader, RawBlockHeader};
    use crate::db::MemoryDB;
    use crate::genesis::EXPORT_SR_40;
    use crate::networks::ChainConfig;
    use crate::shim::address::Address;
    use crate::utils::db::car_util::load_car;
    use nunny::Vec as NonEmpty;
    use std::{io::Cursor, sync::Arc};

    async fn populate_db() -> (NonEmpty<Cid>, Arc<MemoryDB>) {
        let db = Arc::new(MemoryDB::default());
        // The cids are the tipset cids of the most recent tipset (39th)
        let header = load_car(&db, Cursor::new(EXPORT_SR_40)).await.unwrap();
        (header.roots, db)
    }

    #[tokio::test]
    async fn compact_messages_test() {
        let (cids, db) = populate_db().await;

        let gen_block = CachingBlockHeader::new(RawBlockHeader {
            miner_address: Address::new_id(0),
            ..Default::default()
        });

        let response = make_chain_exchange_response(
            &ChainStore::new(
                db.clone(),
                db.clone(),
                db,
                Arc::new(ChainConfig::default()),
                gen_block,
            )
            .unwrap(),
            &ChainExchangeRequest {
                start: cids,
                request_len: 2,
                options: HEADERS | MESSAGES,
            },
        );

        // The response will be loaded with tipsets 39 and 38.
        // See:
        // https://filfox.info/en/tipset/39
        // https://filfox.info/en/tipset/38

        // Response chain should contain 2 tipsets.
        assert_eq!(response.chain.len(), 2);

        let tipset_39_idx = 0;
        let tipset_38_idx = 1;

        let ts_39_msgs = response.chain[tipset_39_idx].messages.as_ref().unwrap();
        let ts_38_msgs = response.chain[tipset_38_idx].messages.as_ref().unwrap();

        // Tipset at height 39...
        // ... has 22 signed messages
        assert_eq!(ts_39_msgs.secp_msgs.len(), 22);
        // ... 12 unsigned messages
        assert_eq!(ts_39_msgs.bls_msgs.len(), 12);
        // Compacted message will contain 1 secp_includes array (since only 1 block in
        // tipset).
        assert_eq!(ts_39_msgs.secp_msg_includes.len(), 1);
        // and 1 bls_includes.
        assert_eq!(ts_39_msgs.bls_msg_includes.len(), 1);

        // Secp_includes will point to all of the signed messages.
        assert_eq!(ts_39_msgs.secp_msg_includes[0].len(), 22);
        // bls_includes will point to all of the unsigned messages.
        assert_eq!(ts_39_msgs.bls_msg_includes[0].len(), 12);

        // Tipset at height 38.
        // ... has 2 blocks
        assert_eq!(response.chain[tipset_38_idx].blocks.len(), 2);
        // ... has same signed and unsigned messages across two blocks. 1 signed total
        assert_eq!(ts_38_msgs.secp_msgs.len(), 1);
        // ... 11 unsigned.
        assert_eq!(ts_38_msgs.bls_msgs.len(), 11);

        // 2 blocks exist in tipset, hence 2 secp_includes and 2 bls_includes
        assert_eq!(ts_38_msgs.secp_msg_includes.len(), 2);
        assert_eq!(ts_38_msgs.bls_msg_includes.len(), 2);

        // Since the messages are duplicated in blocks, each `include` will have them
        // all
        assert_eq!(ts_38_msgs.secp_msg_includes[0].len(), 1);
        assert_eq!(ts_38_msgs.bls_msg_includes[0].len(), 11);
        assert_eq!(ts_38_msgs.secp_msg_includes[1].len(), 1);
        assert_eq!(ts_38_msgs.bls_msg_includes[1].len(), 11);
    }
}

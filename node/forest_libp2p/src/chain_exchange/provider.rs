// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use chain::{ChainStore, Error as ChainError};
use forest_blocks::{Tipset, TipsetKeys};
use forest_cid::Cid;
use ipld_blockstore::BlockStore;
use log::debug;
use std::collections::HashMap;

use super::{
    ChainExchangeRequest, ChainExchangeResponse, ChainExchangeResponseStatus, CompactedMessages,
    TipsetBundle,
};

/// Builds chain exchange response out of chain data.
pub async fn make_chain_exchange_response<DB>(
    cs: &ChainStore<DB>,
    request: &ChainExchangeRequest,
) -> ChainExchangeResponse
where
    DB: BlockStore + Send + Sync + 'static,
{
    let mut response_chain: Vec<TipsetBundle> = Vec::with_capacity(request.request_len as usize);

    let mut curr_tipset_cids = request.start.clone();

    loop {
        let mut tipset_bundle: TipsetBundle = TipsetBundle::default();
        let tipset = match cs
            .tipset_from_keys(&TipsetKeys::new(curr_tipset_cids))
            .await
        {
            Ok(tipset) => tipset,
            Err(err) => {
                debug!("Cannot get tipset from keys: {}", err);

                return ChainExchangeResponse {
                    chain: vec![],
                    status: ChainExchangeResponseStatus::InternalError,
                    message: "Tipset was not found in the database".to_owned(),
                };
            }
        };

        if request.include_messages() {
            match compact_messages(cs.blockstore(), &tipset) {
                Ok(compacted_messages) => tipset_bundle.messages = Some(compacted_messages),
                Err(err) => {
                    debug!("Cannot compact messages for tipset: {}", err);

                    return ChainExchangeResponse {
                        chain: vec![],
                        status: ChainExchangeResponseStatus::InternalError,
                        message: "Can not fullfil the request".to_owned(),
                    };
                }
            }
        }

        curr_tipset_cids = tipset.parents().cids().to_vec();
        let tipset_epoch = tipset.epoch();

        if request.include_blocks() {
            // TODO Cloning blocks isn't ideal, this can maybe be switched to serialize this
            // data in the function. This may not be possible without overriding rpc in libp2p
            tipset_bundle.blocks = tipset.blocks().to_vec();
        }

        response_chain.push(tipset_bundle);

        if response_chain.len() as u64 >= request.request_len || tipset_epoch == 0 {
            break;
        }
    }

    let result_chain_length = response_chain.len() as u64;

    ChainExchangeResponse {
        chain: response_chain,
        status: if result_chain_length < request.request_len {
            ChainExchangeResponseStatus::PartialResponse
        } else {
            ChainExchangeResponseStatus::Success
        },
        message: "Success".to_owned(),
    }
}

// Builds CompactedMessages for given Tipset.
fn compact_messages<DB>(db: &DB, tipset: &Tipset) -> Result<CompactedMessages, ChainError>
where
    DB: BlockStore,
{
    let mut bls_messages_order = HashMap::new();
    let mut secp_messages_order = HashMap::new();
    let mut bls_cids_combined: Vec<Cid> = vec![];
    let mut secp_cids_combined: Vec<Cid> = vec![];
    let mut bls_msg_includes: Vec<Vec<u64>> = vec![];
    let mut secp_msg_includes: Vec<Vec<u64>> = vec![];

    for block_header in tipset.blocks().iter() {
        let (bls_cids, secp_cids) = chain::read_msg_cids(db, block_header.messages())?;

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
        chain::block_messages_from_cids(db, &bls_cids_combined, &secp_cids_combined)?;

    Ok(CompactedMessages {
        bls_msgs,
        bls_msg_includes,
        secp_msgs,
        secp_msg_includes,
    })
}

#[cfg(test)]
mod tests {
    use super::super::{HEADERS, MESSAGES};
    use super::*;
    use async_std::io::BufReader;
    use db::MemoryDB;
    use forest_car::load_car;
    use genesis::EXPORT_SR_40;
    use std::sync::Arc;

    async fn populate_db() -> (Vec<Cid>, MemoryDB) {
        let db = MemoryDB::default();
        let reader = BufReader::<&[u8]>::new(EXPORT_SR_40.as_ref());
        // The cids are the tipset cids of the most recent tipset (39th)
        let cids: Vec<Cid> = load_car(&db, reader).await.unwrap();
        return (cids, db);
    }

    #[async_std::test]
    async fn compact_messages_test() {
        let (cids, db) = populate_db().await;

        let response = make_chain_exchange_response(
            &ChainStore::new(Arc::new(db)),
            &ChainExchangeRequest {
                start: cids,
                request_len: 2,
                options: HEADERS | MESSAGES,
            },
        )
        .await;

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
        // Compacted message will contain 1 secp_includes array (since only 1 block in tipset).
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

        // Since the messages are duplicated in blocks, each `include` will have them all
        assert_eq!(ts_38_msgs.secp_msg_includes[0].len(), 1);
        assert_eq!(ts_38_msgs.bls_msg_includes[0].len(), 11);
        assert_eq!(ts_38_msgs.secp_msg_includes[1].len(), 1);
        assert_eq!(ts_38_msgs.bls_msg_includes[1].len(), 11);
    }
}

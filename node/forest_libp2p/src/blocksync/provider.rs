// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use chain::Error as ChainError;
use forest_blocks::{Tipset, TipsetKeys};
use forest_cid::Cid;
use forest_encoding::tuple::*;
use ipld_blockstore::BlockStore;
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;

use super::{CompactedMessages, TipsetBundle};

/// Blocksync request options
pub const BLOCKS: u64 = 1;
pub const MESSAGES: u64 = 2;
pub const BLOCKS_MESSAGES: u64 = 3;

/// The payload that gets sent to another node to request for blocks and messages. It get DagCBOR serialized before sending over the wire.
#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct BlockSyncRequest {
    /// The tipset to start sync from
    pub start: Vec<Cid>,
    /// The amount of epochs to sync by
    pub request_len: u64,
    /// 1 = Block only, 2 = Messages only, 3 = Blocks and Messages
    pub options: u64,
}

impl BlockSyncRequest {
    pub fn include_blocks(&self) -> bool {
        self.options == BLOCKS || self.options == BLOCKS_MESSAGES
    }

    pub fn include_messages(&self) -> bool {
        self.options == MESSAGES || self.options == BLOCKS_MESSAGES
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BlockSyncResponseStatus {
    // All is well.
    Success = 1,
    // We could not fetch all blocks requested (but at least we returned
    // the `Head` requested). Not considered an error.
    PartialResponse = 101,
    // Request.Start not found.
    BlockNotFound = 201,
    // Requester is making too many requests.
    GoAway = 202,
    // Internal error occured.
    InternalError = 203,
    // Request was bad
    BadRequest = 204,
}

/// The response to a BlockSync request.
#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct BlockSyncResponse {
    /// Error code
    pub status: BlockSyncResponseStatus,
    /// Status message indicating failure reason
    pub message: String,
    /// The tipsets requested
    pub chain: Vec<TipsetBundle>,
}

impl BlockSyncResponse {
    /// Converts blocksync response into result.
    /// Returns an error if the response status is not `Ok`.
    /// Tipset bundle is converted into generic return type with `TryFrom` trait impl.
    pub fn into_result<T>(self) -> Result<Vec<T>, String>
    where
        T: TryFrom<TipsetBundle, Error = String>,
    {
        if self.status != BlockSyncResponseStatus::Success {
            // TODO implement a better error type than string if needed to be handled differently
            return Err(format!("Status {:?}: {}", self.status, self.message));
        }

        self.chain.into_iter().map(T::try_from).collect()
    }
}

/// Builds blocksync response out of chain data.
pub struct BlockSyncProvider<DB: BlockStore> {
    db: Arc<DB>,
}

impl<DB> BlockSyncProvider<DB>
where
    DB: BlockStore,
{
    pub fn new(db: Arc<DB>) -> Self {
        BlockSyncProvider { db }
    }

    /// Builds BlockSyncResponse for an incoming BlockSyncRequest.
    pub fn make_response(&self, request: &BlockSyncRequest) -> BlockSyncResponse {
        let mut response_chain: Vec<TipsetBundle> = vec![];

        let mut curr_tipset_cids = request.start.clone();

        loop {
            let mut tipset_bundle: TipsetBundle = Default::default();
            let tipset =
                match chain::tipset_from_keys(self.db.as_ref(), &TipsetKeys::new(curr_tipset_cids))
                {
                    Ok(tipset) => tipset,
                    Err(err) => {
                        debug!("Cannot get tipset from keys: {}", err);

                        return BlockSyncResponse {
                            chain: vec![],
                            status: BlockSyncResponseStatus::InternalError,
                            message: "Can not fullfil the request".to_owned(),
                        };
                    }
                };

            if request.include_blocks() {
                tipset_bundle.blocks = tipset.blocks().to_vec();
            }

            if request.include_messages() {
                match self.compact_messages(&tipset) {
                    Ok(compacted_messages) => tipset_bundle.messages = Some(compacted_messages),
                    Err(err) => {
                        debug!("Cannot compact messages for tipset: {}", err);

                        return BlockSyncResponse {
                            chain: vec![],
                            status: BlockSyncResponseStatus::InternalError,
                            message: "Can not fullfil the request".to_owned(),
                        };
                    }
                }
            }

            response_chain.push(tipset_bundle);

            if response_chain.len() as u64 >= request.request_len || tipset.epoch() == 0 {
                break;
            }

            curr_tipset_cids = tipset.parents().cids().to_vec();
        }

        let result_chain_length = response_chain.len() as u64;

        BlockSyncResponse {
            chain: response_chain,
            status: if result_chain_length < request.request_len {
                BlockSyncResponseStatus::PartialResponse
            } else {
                BlockSyncResponseStatus::Success
            },
            message: "Success".to_owned(),
        }
    }

    // Builds CompactedMessages for given Tipset.
    fn compact_messages(&self, tipset: &Tipset) -> Result<CompactedMessages, ChainError> {
        let mut bls_messages_order = HashMap::new();
        let mut secp_messages_order = HashMap::new();
        let mut bls_cids_combined: Vec<Cid> = vec![];
        let mut secp_cids_combined: Vec<Cid> = vec![];
        let mut bls_msg_includes: Vec<Vec<u64>> = vec![];
        let mut secp_msg_includes: Vec<Vec<u64>> = vec![];

        for block_header in tipset.blocks().iter() {
            let (bls_cids, secp_cids) =
                chain::read_msg_cids(self.db.as_ref(), block_header.messages())?;

            let mut block_include = vec![];
            let mut secp_include = vec![];

            for bls_cid in bls_cids.iter() {
                let order = match bls_messages_order.get(bls_cid) {
                    Some(order) => *order,
                    None => {
                        let order = block_include.len() as u64;
                        bls_cids_combined.push(bls_cid.clone());
                        bls_messages_order.insert(bls_cid.clone(), order);
                        order
                    }
                };

                block_include.push(order);
            }

            for secp_cid in secp_cids.iter() {
                let order = match secp_messages_order.get(secp_cid) {
                    Some(order) => *order,
                    None => {
                        let order = secp_include.len() as u64;
                        secp_cids_combined.push(secp_cid.clone());
                        secp_messages_order.insert(secp_cid.clone(), order);
                        order
                    }
                };

                secp_include.push(order);
            }

            bls_msg_includes.push(block_include);
            secp_msg_includes.push(secp_include);
        }

        let (bls_msgs, secp_msgs) = chain::block_messages_from_cids(
            self.db.as_ref(),
            &bls_cids_combined,
            &secp_cids_combined,
        )?;

        Ok(CompactedMessages {
            bls_msgs,
            bls_msg_includes,
            secp_msgs,
            secp_msg_includes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use db::MemoryDB;
    use forest_car::load_car;
    use std::io::BufReader;

    fn populate_db() -> (Vec<Cid>, MemoryDB) {
        let db = MemoryDB::default();
        let bz = std::include_bytes!("chain_test.car");
        let reader = BufReader::<&[u8]>::new(bz.as_ref());
        // The cids are the tipset cids of the most recent tipset (39th)
        let cids: Vec<Cid> = load_car(&db, reader).unwrap();
        return (cids, db);
    }

    #[test]
    fn compact_messages_test() {
        let (cids, db) = populate_db();

        let provider = BlockSyncProvider::new(Arc::new(db));

        let response = provider.make_response(&BlockSyncRequest {
            start: cids,
            request_len: 2,
            options: BLOCKS_MESSAGES,
        });

        // The response will be loaded with tipsets 39 and 38.
        // See:
        // https://filfox.info/en/tipset/39
        // https://filfox.info/en/tipset/38

        // Response chain should contain 2 tipsets.
        assert_eq!(response.chain.len(), 2);

        // Tipset at height 39...
        // ... has 22 signed messages
        assert_eq!(
            response.chain[0].messages.as_ref().unwrap().secp_msgs.len(),
            22
        );
        // ... 12 unsigned messages
        assert_eq!(
            response.chain[0].messages.as_ref().unwrap().bls_msgs.len(),
            12
        );
        // Compacted message will contain 1 secp_includes array (since only 1 block in tipset).
        assert_eq!(
            response.chain[0]
                .messages
                .as_ref()
                .unwrap()
                .secp_msg_includes
                .len(),
            1
        );
        // and 1 bls_includes.
        assert_eq!(
            response.chain[0]
                .messages
                .as_ref()
                .unwrap()
                .bls_msg_includes
                .len(),
            1
        );

        // Secp_includes will point to all of the signed messages.
        assert_eq!(
            response.chain[0]
                .messages
                .as_ref()
                .unwrap()
                .secp_msg_includes[0]
                .len(),
            22
        );
        // bls_includes will point to all of the unsigned messages.
        assert_eq!(
            response.chain[0]
                .messages
                .as_ref()
                .unwrap()
                .bls_msg_includes[0]
                .len(),
            12
        );

        // Tipset at height 38.
        // ... has 2 blocks
        assert_eq!(response.chain[1].blocks.len(), 2);
        // ... has same signed and unsigned messages across two blocks. 1 signed total
        assert_eq!(
            response.chain[1].messages.as_ref().unwrap().secp_msgs.len(),
            1
        );
        // ... 11 unsigned.
        assert_eq!(
            response.chain[1].messages.as_ref().unwrap().bls_msgs.len(),
            11
        );

        // 2 blocks exist in tipset, hence 2 secp_includes and 2 bls_includes
        assert_eq!(
            response.chain[1]
                .messages
                .as_ref()
                .unwrap()
                .secp_msg_includes
                .len(),
            2
        );
        assert_eq!(
            response.chain[1]
                .messages
                .as_ref()
                .unwrap()
                .bls_msg_includes
                .len(),
            2
        );

        // Since the messages are duplicated in blocks, each `include` will have them all
        assert_eq!(
            response.chain[1]
                .messages
                .as_ref()
                .unwrap()
                .secp_msg_includes[0]
                .len(),
            1
        );
        assert_eq!(
            response.chain[1]
                .messages
                .as_ref()
                .unwrap()
                .bls_msg_includes[0]
                .len(),
            11
        );
        assert_eq!(
            response.chain[1]
                .messages
                .as_ref()
                .unwrap()
                .secp_msg_includes[1]
                .len(),
            1
        );
        assert_eq!(
            response.chain[1]
                .messages
                .as_ref()
                .unwrap()
                .bls_msg_includes[1]
                .len(),
            11
        );
    }
}

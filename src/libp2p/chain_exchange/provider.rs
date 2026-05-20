// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{io, num::NonZeroUsize, sync::LazyLock};

use ahash::{HashMap, HashMapExt};
use nonzero_ext::nonzero;

use super::{
    ChainExchangeRequest, ChainExchangeResponse, ChainExchangeResponseStatus, CompactedMessages,
    TipsetBundle,
};
use crate::{
    blocks::{Tipset, TipsetKey},
    chain::{ChainStore, Error as ChainError},
    prelude::*,
    utils::misc::env::env_or_default_logged,
};

/// Maximum encoded byte size of a chain-exchange response we serve to peers.
/// Building stops as soon as the running encoded size would exceed this cap;
/// the response is returned with status
/// [`ChainExchangeResponseStatus::PartialResponse`].
static MAX_OUTBOUND_CHAIN_EXCHANGE_RESPONSE_BYTES: LazyLock<NonZeroUsize> = LazyLock::new(|| {
    env_or_default_logged(
        "FOREST_MAX_OUTBOUND_CHAIN_EXCHANGE_RESPONSE_BYTES",
        nonzero!(10 * 1024 * 1024_usize),
    )
});

/// `io::Write` that discards the bytes and only tracks how many were written.
struct CountingSink(usize);

impl io::Write for CountingSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn encoded_size<T: serde::Serialize>(value: &T) -> Result<usize, ChainError> {
    let mut sink = CountingSink(0);
    fvm_ipld_encoding::to_writer(&mut sink, value)?;
    Ok(sink.0)
}

/// Builds chain exchange response out of chain data.
pub fn make_chain_exchange_response(
    cs: &ChainStore,
    request: &ChainExchangeRequest,
) -> ChainExchangeResponse {
    make_chain_exchange_response_with_cap(
        cs,
        request,
        MAX_OUTBOUND_CHAIN_EXCHANGE_RESPONSE_BYTES.get(),
    )
}

fn make_chain_exchange_response_with_cap(
    cs: &ChainStore,
    request: &ChainExchangeRequest,
    max_bytes: usize,
) -> ChainExchangeResponse {
    if !request.is_options_valid() || !request.is_request_len_valid() {
        return ChainExchangeResponse {
            chain: Default::default(),
            status: ChainExchangeResponseStatus::BadRequest,
            message: format!("Invalid chain exchange request {request:?}"),
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

        let mut chain: Vec<TipsetBundle> = Vec::with_capacity(request.request_len as usize);
        let mut accumulated: usize = 0;

        for tipset in root.chain(cs.db()).take(request.request_len as _) {
            let mut tipset_bundle: TipsetBundle = TipsetBundle::default();
            if request.include_messages() {
                tipset_bundle.messages = Some(compact_messages(cs.db(), &tipset)?);
            }
            if request.include_blocks() {
                tipset_bundle.blocks = tipset.block_headers().iter().cloned().collect_vec();
            }

            let bundle_bytes = encoded_size(&tipset_bundle)?;
            // Always include the first bundle so a peer can make forward
            // progress even if a single tipset exceeds the cap.
            if !chain.is_empty() && accumulated + bundle_bytes > max_bytes {
                break;
            }
            accumulated += bundle_bytes;
            chain.push(tipset_bundle);
        }

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

    async fn populate_chain_store() -> (NonEmpty<Cid>, ChainStore) {
        let db = Arc::new(MemoryDB::default());
        // The cids are the tipset cids of the most recent tipset (39th).
        let header = load_car(&db, Cursor::new(EXPORT_SR_40)).await.unwrap();
        let gen_block = CachingBlockHeader::new(RawBlockHeader {
            miner_address: Address::new_id(0),
            ..Default::default()
        });
        let cs = ChainStore::new(db, Arc::new(ChainConfig::default()), gen_block).unwrap();
        (header.roots, cs)
    }

    #[tokio::test]
    async fn compact_messages_test() {
        let (cids, cs) = populate_chain_store().await;

        let response = make_chain_exchange_response(
            &cs,
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

    #[tokio::test]
    async fn response_byte_cap_truncates_to_partial() {
        let (cids, cs) = populate_chain_store().await;

        // A 1-byte cap exercises the always-include-first invariant.
        let response = make_chain_exchange_response_with_cap(
            &cs,
            &ChainExchangeRequest {
                start: cids,
                request_len: 5,
                options: HEADERS | MESSAGES,
            },
            1,
        );

        assert_eq!(response.chain.len(), 1);
        assert_eq!(
            response.status,
            ChainExchangeResponseStatus::PartialResponse
        );
    }

    #[tokio::test]
    async fn counting_sink_matches_to_vec() {
        // Sanity: the sink's running count equals what `to_vec` produces, so the
        // budget we apply is the same byte count we'd actually write on the wire.
        // A real populated response exercises Vec, byte-array, and nested-struct
        // serializations rather than a trivial primitive.
        let (cids, cs) = populate_chain_store().await;
        let response = make_chain_exchange_response(
            &cs,
            &ChainExchangeRequest {
                start: cids,
                request_len: 2,
                options: HEADERS | MESSAGES,
            },
        );

        let mut sink = CountingSink(0);
        fvm_ipld_encoding::to_writer(&mut sink, &response).unwrap();
        let to_vec_len = fvm_ipld_encoding::to_vec(&response).unwrap().len();
        assert!(sink.0 > 0, "expected a non-empty encoded response");
        assert_eq!(sink.0, to_vec_len);
    }
}

// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Tipset;
use crate::cid_collections::CidHashSet;
use crate::ipld::Ipld;
use crate::shim::clock::ChainEpoch;
use crate::utils::db::car_stream::CarBlock;
use crate::utils::encoding::extract_cids;
use crate::utils::multihash::prelude::*;
use cid::Cid;
use futures::Stream;
use fvm_ipld_blockstore::Blockstore;
use pin_project_lite::pin_project;
use std::borrow::Borrow;
use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

fn should_save_block_to_snapshot(cid: Cid) -> bool {
    // Don't include identity `CIDs`.
    // We only include raw and dagcbor, for now.
    // Raw for "code" `CIDs`.
    if cid.hash().code() == u64::from(MultihashCode::Identity) {
        false
    } else {
        matches!(
            cid.codec(),
            crate::shim::crypto::IPLD_RAW | fvm_ipld_encoding::DAG_CBOR
        )
    }
}

/// Depth-first-search iterator for `ipld` leaf nodes.
///
/// This iterator consumes the given `ipld` structure and returns leaf nodes (i.e.,
/// no list or map) in depth-first order. The iterator can be extended at any
/// point by the caller.
///
/// Consider walking this `ipld` graph:
/// ```text
/// List
///  ├ Integer(5)
///  ├ Link(Y)
///  └ String("string")
///
/// Link(Y):
/// Map
///  ├ "key1" => Bool(true)
///  └ "key2" => Float(3.14)
/// ```
///
/// If we walk the above `ipld` graph (replacing `Link(Y)` when it is encountered), the leaf nodes will be seen in this order:
/// 1. `Integer(5)`
/// 2. `Bool(true)`
/// 3. `Float(3.14)`
/// 4. `String("string")`
pub struct DfsIter {
    dfs: VecDeque<Ipld>,
}

impl DfsIter {
    pub fn new(root: Ipld) -> Self {
        DfsIter {
            dfs: VecDeque::from([root]),
        }
    }

    pub fn walk_next(&mut self, ipld: Ipld) {
        self.dfs.push_front(ipld)
    }
}

impl From<Cid> for DfsIter {
    fn from(cid: Cid) -> Self {
        DfsIter::new(Ipld::Link(cid))
    }
}

impl Iterator for DfsIter {
    type Item = Ipld;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(ipld) = self.dfs.pop_front() {
            match ipld {
                Ipld::List(list) => list.into_iter().rev().for_each(|elt| self.walk_next(elt)),
                Ipld::Map(map) => map.into_values().rev().for_each(|elt| self.walk_next(elt)),
                other => return Some(other),
            }
        }
        None
    }
}

enum IterateType {
    Message(Cid),
    StateRoot(Cid),
}

enum Task {
    // Yield the block, don't visit it.
    Emit(Cid, Option<Vec<u8>>),
    // Visit all the elements, recursively.
    Iterate(ChainEpoch, Cid, IterateType, VecDeque<Cid>),
}

pin_project! {
    pub struct ChainStream<DB, T> {
        tipset_iter: T,
        db: DB,
        dfs: VecDeque<Task>, // Depth-first work queue.
        seen: CidHashSet,
        stateroot_limit: ChainEpoch,
        fail_on_dead_links: bool,
    }
}

impl<DB, T> ChainStream<DB, T> {
    pub fn with_seen(mut self, seen: CidHashSet) -> Self {
        self.seen = seen;
        self
    }

    pub fn fail_on_dead_links(mut self, fail_on_dead_links: bool) -> Self {
        self.fail_on_dead_links = fail_on_dead_links;
        self
    }

    #[allow(dead_code)]
    pub fn into_seen(self) -> CidHashSet {
        self.seen
    }
}

/// Stream all blocks that are reachable before the `stateroot_limit` epoch in a depth-first
/// fashion.
/// After this limit, only block headers are streamed. Any dead links are reported as errors.
///
/// # Arguments
///
/// * `db` - A database that implements [`Blockstore`] interface.
/// * `tipset_iter` - An iterator of [`Tipset`], descending order `$child -> $parent`.
/// * `stateroot_limit` - An epoch that signifies how far back we need to inspect tipsets,
///   in-depth. This has to be pre-calculated using this formula: `$cur_epoch - $depth`, where `$depth`
///   is the number of `[`Tipset`]` that needs inspection.
pub fn stream_chain<DB: Blockstore, T: Borrow<Tipset>, ITER: Iterator<Item = T> + Unpin>(
    db: DB,
    tipset_iter: ITER,
    stateroot_limit: ChainEpoch,
) -> ChainStream<DB, ITER> {
    ChainStream {
        tipset_iter,
        db,
        dfs: VecDeque::new(),
        seen: CidHashSet::default(),
        stateroot_limit,
        fail_on_dead_links: true,
    }
}

// Stream available graph in a depth-first search. All reachable nodes are touched and dead-links
// are ignored.
pub fn stream_graph<DB: Blockstore, T: Borrow<Tipset>, ITER: Iterator<Item = T> + Unpin>(
    db: DB,
    tipset_iter: ITER,
    stateroot_limit: ChainEpoch,
) -> ChainStream<DB, ITER> {
    stream_chain(db, tipset_iter, stateroot_limit).fail_on_dead_links(false)
}

impl<DB: Blockstore, T: Borrow<Tipset>, ITER: Iterator<Item = T> + Unpin> Stream
    for ChainStream<DB, ITER>
{
    type Item = anyhow::Result<CarBlock>;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use Task::*;

        let fail_on_dead_links = self.fail_on_dead_links;
        let stateroot_limit = self.stateroot_limit;
        let this = self.project();

        loop {
            while let Some(task) = this.dfs.front_mut() {
                match task {
                    Emit(_, _) => {
                        if let Some(Emit(cid, data)) = this.dfs.pop_front() {
                            if let Some(data) = data {
                                return Poll::Ready(Some(Ok(CarBlock { cid, data })));
                            } else if let Some(data) = this.db.get(&cid)? {
                                return Poll::Ready(Some(Ok(CarBlock { cid, data })));
                            } else if fail_on_dead_links {
                                return Poll::Ready(Some(Err(anyhow::anyhow!(
                                    "[Emit] missing key: {cid}"
                                ))));
                            };
                        }
                    }
                    Iterate(epoch, block_cid, _type, cid_vec) => {
                        while let Some(cid) = cid_vec.pop_front() {
                            // The link traversal implementation assumes there are three types of encoding:
                            // 1. DAG_CBOR: needs to be reachable, so we add it to the queue and load.
                            // 2. IPLD_RAW: WASM blocks, for example. Need to be loaded, but not traversed.
                            // 3. _: ignore all other links
                            // Don't revisit what's already been visited.
                            if should_save_block_to_snapshot(cid) && this.seen.insert(cid) {
                                if let Some(data) = this.db.get(&cid)? {
                                    if cid.codec() == fvm_ipld_encoding::DAG_CBOR {
                                        let new_values = extract_cids(&data)?;
                                        if !new_values.is_empty() {
                                            cid_vec.reserve(new_values.len());
                                            for v in new_values.into_iter().rev() {
                                                cid_vec.push_front(v)
                                            }
                                        }
                                    }
                                    return Poll::Ready(Some(Ok(CarBlock { cid, data })));
                                } else if fail_on_dead_links {
                                    let type_display = match _type {
                                        IterateType::Message(c) => {
                                            format!("message {c}")
                                        }
                                        IterateType::StateRoot(c) => {
                                            format!("state root {c}")
                                        }
                                    };
                                    return Poll::Ready(Some(Err(anyhow::anyhow!(
                                        "[Iterate] missing key: {cid} from {type_display} in block {block_cid} at epoch {epoch}"
                                    ))));
                                }
                            }
                        }
                        this.dfs.pop_front();
                    }
                }
            }

            // This consumes a [`Tipset`] from the iterator one at a time. The next iteration of the
            // enclosing loop is processing the queue. Once the desired depth has been reached -
            // yield the block without walking the graph it represents.
            if let Some(tipset) = this.tipset_iter.next() {
                for block in tipset.borrow().block_headers() {
                    let (cid, data) = block.car_block()?;
                    if this.seen.insert(cid) {
                        // Make sure we always yield a block otherwise.
                        this.dfs.push_back(Emit(cid, Some(data)));

                        if block.epoch == 0 {
                            // The genesis block has some kind of dummy parent that needs to be emitted.
                            for p in &block.parents {
                                this.dfs.push_back(Emit(p, None));
                            }
                        }

                        // Process block messages.
                        if block.epoch > stateroot_limit {
                            this.dfs.push_back(Iterate(
                                block.epoch,
                                *block.cid(),
                                IterateType::Message(block.messages),
                                DfsIter::from(block.messages)
                                    .filter_map(ipld_to_cid)
                                    .collect(),
                            ));
                        }

                        // Visit the block if it's within required depth. And a special case for `0`
                        // epoch to match Lotus' implementation.
                        if block.epoch == 0 || block.epoch > stateroot_limit {
                            // NOTE: In the original `walk_snapshot` implementation we walk the dag
                            // immediately. Which is what we do here as well, but using a queue.
                            this.dfs.push_back(Iterate(
                                block.epoch,
                                *block.cid(),
                                IterateType::StateRoot(block.state_root),
                                DfsIter::from(block.state_root)
                                    .filter_map(ipld_to_cid)
                                    .collect(),
                            ));
                        }
                    }
                }
            } else {
                // That's it, nothing else to do. End of stream.
                return Poll::Ready(None);
            }
        }
    }
}

fn ipld_to_cid(ipld: Ipld) -> Option<Cid> {
    if let Ipld::Link(cid) = ipld {
        Some(cid)
    } else {
        None
    }
}

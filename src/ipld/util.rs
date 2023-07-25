// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    collections::VecDeque,
    future::Future,
    sync::{
        atomic::{self, AtomicU64},
        Arc,
    },
};

use crate::blocks::{BlockHeader, Tipset};
use crate::utils::io::progress_log::WithProgressRaw;
use cid::Cid;
use fvm_ipld_encoding::from_slice;
use lazy_static::lazy_static;

use crate::ipld::{CidHashSet, Ipld};

/// Traverses all Cid links, hashing and loading all unique values and using the
/// callback function to interact with the data.
#[async_recursion::async_recursion]
async fn traverse_ipld_links_hash<F, T>(
    walked: &mut CidHashSet,
    load_block: &mut F,
    ipld: &Ipld,
    on_inserted: &(impl Fn(usize) + Send + Sync),
) -> Result<(), anyhow::Error>
where
    F: FnMut(Cid) -> T + Send,
    T: Future<Output = Result<Vec<u8>, anyhow::Error>> + Send,
{
    match ipld {
        Ipld::Map(m) => {
            for (_, v) in m.iter() {
                traverse_ipld_links_hash(walked, load_block, v, on_inserted).await?;
            }
        }
        Ipld::List(list) => {
            for v in list.iter() {
                traverse_ipld_links_hash(walked, load_block, v, on_inserted).await?;
            }
        }
        &Ipld::Link(cid) => {
            // WASM blocks are stored as IPLD_RAW. They should be loaded but not traversed.
            if cid.codec() == crate::shim::crypto::IPLD_RAW {
                if !walked.insert(cid) {
                    return Ok(());
                }
                on_inserted(walked.len());
                let _ = load_block(cid).await?;
            }
            if cid.codec() == fvm_ipld_encoding::DAG_CBOR {
                if !walked.insert(cid) {
                    return Ok(());
                }
                on_inserted(walked.len());
                let bytes = load_block(cid).await?;
                let ipld = from_slice(&bytes)?;
                traverse_ipld_links_hash(walked, load_block, &ipld, on_inserted).await?;
            }
        }
        _ => (),
    }
    Ok(())
}

/// Load and hash CIDs and resolve recursively.
pub async fn recurse_links_hash<F, T>(
    walked: &mut CidHashSet,
    root: Cid,
    load_block: &mut F,
    on_inserted: &(impl Fn(usize) + Send + Sync),
) -> Result<(), anyhow::Error>
where
    F: FnMut(Cid) -> T + Send,
    T: Future<Output = Result<Vec<u8>, anyhow::Error>> + Send,
{
    if !walked.insert(root) {
        // Cid has already been traversed
        return Ok(());
    }
    on_inserted(walked.len());
    if root.codec() != fvm_ipld_encoding::DAG_CBOR {
        return Ok(());
    }

    let bytes = load_block(root).await?;
    let ipld = from_slice(&bytes)?;

    traverse_ipld_links_hash(walked, load_block, &ipld, on_inserted).await?;

    Ok(())
}

pub type ProgressBarCurrentTotalPair = Arc<(AtomicU64, AtomicU64)>;

lazy_static! {
    pub static ref WALK_SNAPSHOT_PROGRESS_DB_GC: ProgressBarCurrentTotalPair = Default::default();
}

/// Walks over tipset and state data and loads all blocks not yet seen.
/// This is tracked based on the callback function loading blocks.
pub async fn walk_snapshot<F, T>(
    tipset: &Tipset,
    recent_roots: i64,
    mut load_block: F,
    progress_bar_message: Option<&str>,
    progress_tracker: Option<ProgressBarCurrentTotalPair>,
    estimated_total_records: Option<u64>,
) -> anyhow::Result<usize>
where
    F: FnMut(Cid) -> T + Send,
    T: Future<Output = anyhow::Result<Vec<u8>>> + Send,
{
    let estimated_total_records = estimated_total_records.unwrap_or_default();
    let message = progress_bar_message.unwrap_or("Walking snapshot");
    #[allow(deprecated)] // Tracking issue: https://github.com/ChainSafe/forest/issues/3157
    let wp = WithProgressRaw::new(message, estimated_total_records);

    let mut seen = CidHashSet::default();
    let mut blocks_to_walk: VecDeque<Cid> = tipset.cids().to_vec().into();
    let mut current_min_height = tipset.epoch();
    let incl_roots_epoch = tipset.epoch() - recent_roots;

    let on_inserted = {
        let wp = wp.clone();
        let progress_tracker = progress_tracker.clone();
        move |len: usize| {
            let progress = len as u64;
            let total = progress.max(estimated_total_records);
            wp.set(progress);
            wp.set_total(total);
            if let Some(progress_tracker) = &progress_tracker {
                progress_tracker
                    .0
                    .store(progress, atomic::Ordering::Relaxed);
                progress_tracker.1.store(total, atomic::Ordering::Relaxed);
            }
        }
    };

    while let Some(next) = blocks_to_walk.pop_front() {
        if !seen.insert(next) {
            continue;
        };
        on_inserted(seen.len());

        if !should_save_block_to_snapshot(next) {
            continue;
        }

        let data = load_block(next).await?;
        let h = from_slice::<BlockHeader>(&data)?;

        if current_min_height > h.epoch() {
            current_min_height = h.epoch();
        }

        if h.epoch() > incl_roots_epoch {
            recurse_links_hash(&mut seen, *h.messages(), &mut load_block, &on_inserted).await?;
        }

        if h.epoch() > 0 {
            for p in h.parents().cids() {
                blocks_to_walk.push_back(*p);
            }
        } else {
            for p in h.parents().cids() {
                load_block(*p).await?;
            }
        }

        if h.epoch() == 0 || h.epoch() > incl_roots_epoch {
            recurse_links_hash(&mut seen, *h.state_root(), &mut load_block, &on_inserted).await?;
        }
    }

    Ok(seen.len())
}

fn should_save_block_to_snapshot(cid: Cid) -> bool {
    // Don't include identity CIDs.
    // We only include raw and dagcbor, for now.
    // Raw for "code" CIDs.
    if cid.hash().code() == u64::from(cid::multihash::Code::Identity) {
        false
    } else {
        matches!(
            cid.codec(),
            crate::shim::crypto::IPLD_RAW | fvm_ipld_encoding::DAG_CBOR
        )
    }
}

use crate::shim::clock::ChainEpoch;
use futures::Stream;
use fvm_ipld_blockstore::Blockstore;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};

enum Task {
    // Yield the block, don't visit it.
    Emit(Cid),
    // Visit all the elements, recursively.
    Iterate(Ipld),
}

pin_project! {
    struct ChainStream<DB, T> {
        #[pin]
        tipset_iter: T,
        db: DB,
        dfs: VecDeque<Task>, // Depth-first work queue.
        seen: CidHashSet,
        stateroot_limit: ChainEpoch,
    }
}

/// Initializes a stream of blocks.
///
/// # Arguments
///
/// * `db` - A database that implements [`Blockstore`] interface.
/// * `tipset_iter` - An iterator of [`Tipset`], descending order `$child -> $parent`.
/// * `stateroot_limit` - An epoch that signifies how far back we need to inspect tipsets.
/// in-depth. This has to be pre-calculated using this formula: `$cur_epoch - $depth`, where
/// `$depth` is the number of `[`Tipset`]` that needs inspection.
pub fn stream_chain<DB: Blockstore, T: Iterator<Item = Tipset> + Unpin>(
    db: DB,
    tipset_iter: T,
    stateroot_limit: ChainEpoch,
) -> impl Stream<Item = anyhow::Result<(Cid, Vec<u8>)>> {
    ChainStream {
        tipset_iter,
        db,
        dfs: VecDeque::new(),
        seen: CidHashSet::default(),
        stateroot_limit,
    }
}

impl<DB: Blockstore, T: Iterator<Item = Tipset> + Unpin> Stream for ChainStream<DB, T> {
    type Item = anyhow::Result<(Cid, Vec<u8>)>;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use Task::*;
        let mut this = self.project();

        let stateroot_limit = *this.stateroot_limit;
        loop {
            while let Some(ipld) = this.dfs.pop_front() {
                match ipld {
                    Emit(cid) => {
                        let result = this.db.get(&cid);

                        let result = result.and_then(|val| {
                            let block = val.ok_or(anyhow::anyhow!("missing key"))?;
                            Ok((cid, block))
                        });

                        return Poll::Ready(Some(result));
                    }
                    Iterate(Ipld::Null) => {}
                    Iterate(Ipld::Bool(_)) => {}
                    Iterate(Ipld::Integer(_)) => {}
                    Iterate(Ipld::Float(_)) => {}
                    Iterate(Ipld::String(_)) => {}
                    Iterate(Ipld::Bytes(_)) => {}
                    Iterate(Ipld::List(list)) => list
                        .into_iter()
                        .rev()
                        .for_each(|elt| this.dfs.push_front(Iterate(elt))),
                    Iterate(Ipld::Map(map)) => map
                        .into_values()
                        .rev()
                        .for_each(|elt| this.dfs.push_front(Iterate(elt))),
                    // The link traversal implementation assumes there are three types of encoding:
                    // 1. DAG_CBOR: needs to be reachable, so we add it to the queue and load.
                    // 2. IPLD_RAW: WASM blocks, for example. Need to be loaded, but not traversed.
                    // 3. _: ignore all other links
                    Iterate(Ipld::Link(cid)) => {
                        // Don't revisit what's already been visited.
                        if should_save_block_to_snapshot(cid) && this.seen.insert(cid) {
                            let result = this.db.get(&cid);

                            let result = result.and_then(|val| {
                                let block = val.ok_or(anyhow::anyhow!("missing key"))?;
                                if cid.codec() == fvm_ipld_encoding::DAG_CBOR {
                                    let ipld: Ipld = from_slice(&block)?;
                                    this.dfs.push_front(Iterate(ipld));
                                }
                                Ok((cid, block))
                            });

                            return Poll::Ready(Some(result));
                        }
                    }
                }
            }

            // This consumes a [`Tipset`] from the iterator one at a time. The next iteration of the
            // enclosing loop is processing the queue. Once the desired depth has been reached -
            // yield the block without walking the graph it represents.
            if let Some(tipset) = this.tipset_iter.as_mut().next() {
                for block in tipset.into_blocks().into_iter() {
                    // Make sure we always yield a block otherwise.
                    this.dfs.push_back(Emit(*block.cid()));

                    if block.epoch() == 0 {
                        // The genesis block has some kind of dummy parent that needs to be emitted.
                        for p in block.parents().cids() {
                            this.dfs.push_back(Emit(*p));
                        }
                    }

                    // Process block messages.
                    if block.epoch() > stateroot_limit {
                        this.dfs.push_back(Iterate(Ipld::Link(*block.messages())));
                    }

                    // Visit the block if it's within required depth. And a special case for `0`
                    // epoch to match Lotus' implementation.
                    if block.epoch() == 0 || block.epoch() > stateroot_limit {
                        // NOTE: In the original `walk_snapshot` implementation we walk the dag
                        // immediately. Which is what we do here as well, but using a queue.
                        this.dfs.push_back(Iterate(Ipld::Link(*block.state_root())));
                    }
                }
            } else {
                // That's it, nothing else to do. End of stream.
                return Poll::Ready(None);
            }
        }
    }
}

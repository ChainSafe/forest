// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{collections::VecDeque, future::Future, sync::Arc};

use crate::cid_collections::CidHashSet;
use crate::ipld::Ipld;
use crate::shim::clock::ChainEpoch;
use crate::utils::db::car_stream::CarBlock;
use crate::utils::encoding::extract_cids;
use crate::{blocks::Tipset, utils::encoding::from_slice_with_fallback};
use anyhow::Context as _;
use cid::Cid;
use futures::Stream;
use fvm_ipld_blockstore::Blockstore;
use parking_lot::Mutex;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::task;
use tokio::task::{JoinHandle, JoinSet};

const BLOCK_CHANNEL_LIMIT: usize = 2048;

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
                let ipld = from_slice_with_fallback(&bytes)?;
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
    let ipld = from_slice_with_fallback(&bytes)?;

    traverse_ipld_links_hash(walked, load_block, &ipld, on_inserted).await?;

    Ok(())
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

enum Task {
    // Yield the block, don't visit it.
    Emit(Cid),
    // Visit all the elements, recursively.
    Iterate(VecDeque<Cid>),
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
    pub fn with_seen(self, seen: CidHashSet) -> Self {
        ChainStream { seen, ..self }
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
/// in-depth. This has to be pre-calculated using this formula: `$cur_epoch - $depth`, where `$depth`
/// is the number of `[`Tipset`]` that needs inspection.
pub fn stream_chain<DB: Blockstore, T: Iterator<Item = Tipset> + Unpin>(
    db: DB,
    tipset_iter: T,
    stateroot_limit: ChainEpoch,
) -> ChainStream<DB, T> {
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
pub fn stream_graph<DB: Blockstore, T: Iterator<Item = Tipset> + Unpin>(
    db: DB,
    tipset_iter: T,
    stateroot_limit: ChainEpoch,
) -> ChainStream<DB, T> {
    ChainStream {
        tipset_iter,
        db,
        dfs: VecDeque::new(),
        seen: CidHashSet::default(),
        stateroot_limit,
        fail_on_dead_links: false,
    }
}

impl<DB: Blockstore, T: Iterator<Item = Tipset> + Unpin> Stream for ChainStream<DB, T> {
    type Item = anyhow::Result<CarBlock>;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use Task::*;
        let this = self.project();

        let ipld_to_cid = |ipld| {
            if let Ipld::Link(cid) = ipld {
                return Some(cid);
            }
            None
        };

        let stateroot_limit = *this.stateroot_limit;
        loop {
            while let Some(task) = this.dfs.front_mut() {
                match task {
                    Emit(cid) => {
                        let cid = *cid;
                        this.dfs.pop_front();
                        if let Some(data) = this.db.get(&cid)? {
                            return Poll::Ready(Some(Ok(CarBlock { cid, data })));
                        } else if *this.fail_on_dead_links {
                            return Poll::Ready(Some(Err(anyhow::anyhow!("missing key: {}", cid))));
                        }
                    }
                    Iterate(cid_vec) => {
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
                                        cid_vec.reserve(new_values.len());

                                        for v in new_values.into_iter().rev() {
                                            cid_vec.push_front(v)
                                        }
                                    }
                                    return Poll::Ready(Some(Ok(CarBlock { cid, data })));
                                } else if *this.fail_on_dead_links {
                                    return Poll::Ready(Some(Err(anyhow::anyhow!(
                                        "missing key: {}",
                                        cid
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
                for block in tipset.into_blocks().into_iter() {
                    if this.seen.insert(*block.cid()) {
                        // Make sure we always yield a block otherwise.
                        this.dfs.push_back(Emit(*block.cid()));

                        if block.epoch() == 0 {
                            // The genesis block has some kind of dummy parent that needs to be emitted.
                            for p in block.parents().cids.clone() {
                                this.dfs.push_back(Emit(p));
                            }
                        }

                        // Process block messages.
                        if block.epoch() > stateroot_limit {
                            this.dfs.push_back(Iterate(
                                DfsIter::from(*block.messages())
                                    .filter_map(ipld_to_cid)
                                    .collect(),
                            ));
                        }

                        // Visit the block if it's within required depth. And a special case for `0`
                        // epoch to match Lotus' implementation.
                        if block.epoch() == 0 || block.epoch() > stateroot_limit {
                            // NOTE: In the original `walk_snapshot` implementation we walk the dag
                            // immediately. Which is what we do here as well, but using a queue.
                            this.dfs.push_back(Iterate(
                                DfsIter::from(*block.state_root())
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

pin_project! {
    pub struct UnorderedChainStream<DB, T> {
        tipset_iter: T,
        db: Arc<DB>,
        seen: Arc<Mutex<CidHashSet>>,
        worker_handle: JoinHandle<anyhow::Result<()>>,
        block_receiver: flume::Receiver<anyhow::Result<CarBlock>>,
        extract_sender: flume::Sender<Cid>,
        stateroot_limit: ChainEpoch,
        queue: Vec<Cid>,
        fail_on_dead_links: bool,
    }
}

impl<DB, T> UnorderedChainStream<DB, T> {
    pub fn into_seen(self) -> CidHashSet {
        match Arc::try_unwrap(self.seen) {
            Ok(v) => v.into_inner(),
            Err(v) => v.lock().clone(),
        }
    }

    #[allow(dead_code)]
    pub async fn join_workers(self) -> anyhow::Result<()> {
        self.worker_handle.await?
    }
}

/// Stream all blocks that are reachable before the `stateroot_limit` epoch in an unordered fashion.
/// After this limit, only block headers are streamed. Any dead links are reported as errors.
///
/// # Arguments
///
/// * `db` - A database that implements [`Blockstore`] interface.
/// * `tipset_iter` - An iterator of [`Tipset`], descending order `$child -> $parent`.
/// * `stateroot_limit` - An epoch that signifies how far back we need to inspect tipsets, in-depth.
/// This has to be pre-calculated using this formula: `$cur_epoch - $depth`, where `$depth` is the
/// number of `[`Tipset`]` that needs inspection.
#[allow(dead_code)]
pub fn unordered_stream_chain<
    DB: Blockstore + Sync + Send + 'static,
    T: Iterator<Item = Tipset> + Unpin + Send + 'static,
>(
    db: Arc<DB>,
    tipset_iter: T,
    stateroot_limit: ChainEpoch,
) -> UnorderedChainStream<DB, T> {
    let (sender, receiver) = flume::bounded(BLOCK_CHANNEL_LIMIT);
    let (extract_sender, extract_receiver) = flume::unbounded();
    let fail_on_dead_links = true;
    let seen = Arc::new(Mutex::new(CidHashSet::default()));
    let handle = UnorderedChainStream::<DB, T>::start_workers(
        db.clone(),
        sender.clone(),
        extract_receiver,
        seen.clone(),
        fail_on_dead_links,
    );

    UnorderedChainStream {
        seen,
        db,
        worker_handle: handle,
        block_receiver: receiver,
        queue: Vec::new(),
        extract_sender,
        tipset_iter,
        stateroot_limit,
        fail_on_dead_links,
    }
}

// Stream available graph in unordered search. All reachable nodes are touched and dead-links
// are ignored.
pub fn unordered_stream_graph<
    DB: Blockstore + Sync + Send + 'static,
    T: Iterator<Item = Tipset> + Unpin + Send + 'static,
>(
    db: Arc<DB>,
    tipset_iter: T,
    stateroot_limit: ChainEpoch,
) -> UnorderedChainStream<DB, T> {
    let (sender, receiver) = flume::bounded(BLOCK_CHANNEL_LIMIT);
    let (extract_sender, extract_receiver) = flume::unbounded();
    let fail_on_dead_links = false;
    let seen = Arc::new(Mutex::new(CidHashSet::default()));
    let handle = UnorderedChainStream::<DB, T>::start_workers(
        db.clone(),
        sender.clone(),
        extract_receiver,
        seen.clone(),
        fail_on_dead_links,
    );

    UnorderedChainStream {
        seen,
        db,
        worker_handle: handle,
        block_receiver: receiver,
        queue: Vec::new(),
        tipset_iter,
        extract_sender,
        stateroot_limit,
        fail_on_dead_links,
    }
}

impl<DB: Blockstore + Send + Sync + 'static, T: Iterator<Item = Tipset> + Unpin>
    UnorderedChainStream<DB, T>
{
    fn start_workers(
        db: Arc<DB>,
        block_sender: flume::Sender<anyhow::Result<CarBlock>>,
        extract_receiver: flume::Receiver<Cid>,
        seen: Arc<Mutex<CidHashSet>>,
        fail_on_dead_links: bool,
    ) -> JoinHandle<anyhow::Result<()>> {
        task::spawn(async move {
            let mut handles = JoinSet::new();

            for _ in 0..num_cpus::get() {
                let seen = seen.clone();
                let extract_receiver = extract_receiver.clone();
                let db = db.clone();
                let block_sender = block_sender.clone();
                handles.spawn(async move {
                    while let Ok(cid) = extract_receiver.recv_async().await {
                        let mut cid_vec = vec![cid];
                        while let Some(cid) = cid_vec.pop() {
                            if should_save_block_to_snapshot(cid) && seen.lock().insert(cid) {
                                if let Some(data) = db.get(&cid)? {
                                    if cid.codec() == fvm_ipld_encoding::DAG_CBOR {
                                        let mut new_values = extract_cids(&data)?;
                                        cid_vec.append(&mut new_values);
                                    }
                                    block_sender
                                        .send(Ok(CarBlock { cid, data }))
                                        .expect("unreachable");
                                } else if fail_on_dead_links {
                                    block_sender
                                        .send(Err(anyhow::anyhow!("missing key: {}", cid)))
                                        .expect("unreachable");
                                    return anyhow::Ok(());
                                }
                            }
                        }
                    }
                    anyhow::Ok(())
                });
            }

            // Make sure we report any unexpected errors.
            while let Some(res) = handles.join_next().await {
                match res {
                    Ok(_) => continue,
                    Err(err) if err.is_cancelled() => continue,
                    Err(err) => return Err(err).context("worker error"),
                }
            }
            Ok(())
        })
    }
}

impl<DB: Blockstore + Send + Sync + 'static, T: Iterator<Item = Tipset> + Unpin> Stream
    for UnorderedChainStream<DB, T>
{
    type Item = anyhow::Result<CarBlock>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        loop {
            while let Some(cid) = this.queue.pop() {
                if let Some(data) = this.db.get(&cid)? {
                    return Poll::Ready(Some(Ok(CarBlock { cid, data })));
                } else if *this.fail_on_dead_links {
                    // Let the workers go on error.
                    this.extract_sender.downgrade();
                    return Poll::Ready(Some(Err(anyhow::anyhow!("missing key: {}", cid))));
                }
            }

            // We don't care if the channel is empty/closed - that's being checked below.
            if let Ok(block) = this.block_receiver.try_recv() {
                // Make sure we let the workers go when an error is encountered.
                if block.is_err() {
                    this.extract_sender.downgrade();
                }
                return Poll::Ready(Some(block));
            }

            let stateroot_limit = *this.stateroot_limit;
            // This consumes a [`Tipset`] from the iterator one at a time. Workers are then processing
            // the extract queue. The emit queue is processed in the loop above. Once the desired depth
            // has been reached yield a block without walking the graph it represents.
            if let Some(tipset) = this.tipset_iter.next() {
                for block in tipset.into_blocks().into_iter() {
                    if this.seen.lock().insert(*block.cid()) {
                        // Make sure we always yield a block, directly to the stream to avoid extra
                        // work.
                        this.queue.push(*block.cid());

                        if block.epoch() == 0 {
                            // The genesis block has some kind of dummy parent that needs to be emitted.
                            for p in block.parents().cids.clone() {
                                this.queue.push(p);
                            }
                        }

                        // Process block messages.
                        if block.epoch() > stateroot_limit
                            && should_save_block_to_snapshot(*block.messages())
                        {
                            if this.db.has(block.messages())? {
                                this.extract_sender.send(*block.messages())?;
                                // This will simply return an error once we reach that item in
                                // the queue.
                            } else if *this.fail_on_dead_links {
                                this.queue.push(*block.messages());
                            } else {
                                // Make sure we update seen here as we don't send the block for
                                // inspection.
                                this.seen.lock().insert(*block.messages());
                            }
                        }

                        // Visit the block if it's within required depth. And a special case for `0`
                        // epoch to match Lotus' implementation.
                        if (block.epoch() == 0 || block.epoch() > stateroot_limit)
                            && should_save_block_to_snapshot(*block.state_root())
                        {
                            if this.db.has(block.state_root())? {
                                this.extract_sender.send(*block.state_root())?;
                                // This will simply return an error once we reach that item in
                                // the queue.
                            } else if *this.fail_on_dead_links {
                                this.queue.push(*block.state_root());
                            } else {
                                // Make sure we update seen here as we don't send the block for
                                // inspection.
                                this.seen.lock().insert(*block.state_root());
                            }
                        }
                    }
                }
            } else if !this.block_receiver.is_disconnected() {
                if let Ok(item) = this.block_receiver.try_recv() {
                    return Poll::Ready(Some(item));
                }
                // Close the sender when it's empty, then we can exit in the next iteration or when
                // the block_receiver is empty, in case there are some in-flight messages.
                if this.extract_sender.is_empty() && this.block_receiver.is_empty() {
                    this.extract_sender.downgrade();
                }
            } else {
                // That's it, nothing else to do. End of stream.
                return Poll::Ready(None);
            };
        }
    }
}

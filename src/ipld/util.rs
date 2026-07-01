// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Tipset;
use crate::cid_collections::{CidHashSet, CidHashSetLike};
use crate::ipld::Ipld;
use crate::prelude::*;
use crate::shim::clock::ChainEpoch;
use crate::shim::executor::Receipt;
use crate::utils::db::car_stream::CarBlock;
use crate::utils::encoding::extract_cids;
use crate::utils::multihash::prelude::*;
use arc_swap::ArcSwapOption;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::Stream;
use pin_project_lite::pin_project;
use std::borrow::Borrow;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicI64};
use std::sync::{LazyLock, atomic};
use std::task::{Context, Poll};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct ExportStatus {
    pub epoch: AtomicI64,
    pub initial_epoch: AtomicI64,
    pub exporting: AtomicBool,
    pub cancelled: AtomicBool,
    pub start_time: ArcSwapOption<DateTime<Utc>>,
    pub cancellation_token: ArcSwapOption<CancellationToken>,
}

impl ExportStatus {
    pub fn epoch(&self) -> i64 {
        self.epoch.load(atomic::Ordering::Relaxed)
    }

    pub fn initial_epoch(&self) -> i64 {
        self.initial_epoch.load(atomic::Ordering::Relaxed)
    }

    pub fn exporting(&self) -> bool {
        self.exporting.load(atomic::Ordering::Relaxed)
    }

    pub fn cancelled(&self) -> bool {
        self.cancelled.load(atomic::Ordering::Relaxed)
    }

    pub fn start_time(&self) -> Option<DateTime<Utc>> {
        self.start_time.load().clone().map(Arc::unwrap_or_clone)
    }

    pub fn cancellation_token(&self) -> Option<CancellationToken> {
        self.cancellation_token
            .load()
            .clone()
            .map(Arc::unwrap_or_clone)
    }
}

pub static CHAIN_EXPORT_STATUS: LazyLock<ExportStatus> = LazyLock::new(ExportStatus::default);

pub struct ChainExportGuard {
    cancellation_token: CancellationToken,
}

impl ChainExportGuard {
    pub fn try_start_export() -> anyhow::Result<Self> {
        let cancellation_token = CancellationToken::new();
        start_export(cancellation_token.clone())?;
        Ok(Self { cancellation_token })
    }

    pub fn cancel_export(&self) {
        cancel_export()
    }

    pub fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation_token
    }
}

impl Drop for ChainExportGuard {
    fn drop(&mut self) {
        // In case some tasks are waiting on this token
        self.cancellation_token.cancel();
        end_export()
    }
}

fn update_epoch(new_value: i64) {
    let status = &*CHAIN_EXPORT_STATUS;
    status.epoch.store(new_value, atomic::Ordering::Relaxed);
    _ = status.initial_epoch.compare_exchange(
        0,
        new_value,
        atomic::Ordering::Relaxed,
        atomic::Ordering::Relaxed,
    );
}

fn start_export(cancellation_token: CancellationToken) -> anyhow::Result<()> {
    let status = &*CHAIN_EXPORT_STATUS;
    let export_in_progress = status.exporting.swap(true, atomic::Ordering::Relaxed);
    anyhow::ensure!(
        !export_in_progress,
        "An active chain export job has started at {}, start epoch: {}, current epoch: {}",
        status.start_time().unwrap_or_default(),
        status.initial_epoch(),
        status.epoch(),
    );
    status.epoch.store(0, atomic::Ordering::Relaxed);
    status.initial_epoch.store(0, atomic::Ordering::Relaxed);
    status.cancelled.store(false, atomic::Ordering::Relaxed);
    status.start_time.store(Some(Utc::now().into()));
    status
        .cancellation_token
        .store(Some(cancellation_token.into()));
    Ok(())
}

fn end_export() {
    CHAIN_EXPORT_STATUS
        .exporting
        .store(false, atomic::Ordering::Relaxed);
    CHAIN_EXPORT_STATUS.cancellation_token.store(None);
}

fn cancel_export() {
    let status = &*CHAIN_EXPORT_STATUS;
    status.exporting.store(false, atomic::Ordering::Relaxed);
    status.cancelled.store(true, atomic::Ordering::Relaxed);
}

fn should_save_block_to_snapshot(cid: Cid) -> bool {
    // Don't include identity CIDs.
    // We only include raw and dagcbor, for now.
    // Raw for "code" CIDs.
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
    dfs: Vec<Ipld>,
}

impl DfsIter {
    pub fn new(root: Ipld) -> Self {
        DfsIter { dfs: vec![root] }
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
        while let Some(ipld) = self.dfs.pop() {
            match ipld {
                Ipld::List(list) => self.dfs.extend(list.into_iter().rev()),
                Ipld::Map(map) => self.dfs.extend(map.into_values().rev()),
                other => return Some(other),
            }
        }
        None
    }
}

enum IterateType {
    Message(Cid),
    MessageReceipts(Cid),
    StateRoot(Cid),
    EventsRoot(Cid),
}

enum Task {
    // Yield the block, don't visit it.
    Emit(Cid, Option<Bytes>),
    // Visit all the elements, recursively.
    Iterate(ChainEpoch, Cid, IterateType, Vec<Cid>),
}

pin_project! {
    pub struct ChainStream<DB, T, S = CidHashSet> {
        tipset_iter: T,
        db: DB,
        dfs: VecDeque<Task>, // Depth-first work queue.
        seen: S,
        stateroot_limit_exclusive: ChainEpoch,
        fail_on_dead_links: bool,
        message_receipts: bool,
        events: bool,
        tipset_keys:bool,
        track_progress: bool,
        n_polled: usize,
    }
}

impl<DB, T, S> ChainStream<DB, T, S> {
    pub fn fail_on_dead_links(mut self, fail_on_dead_links: bool) -> Self {
        self.fail_on_dead_links = fail_on_dead_links;
        self
    }

    pub fn track_progress(mut self, track_progress: bool) -> Self {
        self.track_progress = track_progress;
        self
    }

    /// Whether to enable traversal of message receipt roots during chain export.
    pub fn with_message_receipts(mut self, message_receipts: bool) -> Self {
        self.message_receipts = message_receipts;
        self
    }

    /// Whether to enable traversal of events roots during chain export.
    /// Requires message receipts to be enabled as well.
    pub fn with_events(mut self, events: bool) -> Self {
        self.events = events;
        self
    }

    /// Whether to export tipset keys.
    pub fn with_tipset_keys(mut self, tipset_keys: bool) -> Self {
        self.tipset_keys = tipset_keys;
        self
    }

    pub fn into_seen(self) -> S {
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
/// * `stateroot_limit` - An epoch that signifies how far back (exclusive) we need to inspect tipsets,
///   in-depth. This has to be pre-calculated using this formula: `$cur_epoch - $depth`, where `$depth`
///   is the number of `[`Tipset`]` that needs inspection.
pub fn stream_chain<
    DB: Blockstore,
    T: Borrow<Tipset>,
    ITER: Iterator<Item = T> + Unpin,
    S: CidHashSetLike,
>(
    db: DB,
    tipset_iter: ITER,
    stateroot_limit_exclusive: ChainEpoch,
    seen: S,
) -> ChainStream<DB, ITER, S> {
    ChainStream {
        tipset_iter,
        db,
        dfs: VecDeque::new(),
        seen,
        stateroot_limit_exclusive,
        fail_on_dead_links: true,
        message_receipts: false,
        events: false,
        tipset_keys: false,
        track_progress: false,
        n_polled: 0,
    }
}

// Stream available graph in a depth-first search. All reachable nodes are touched and dead-links
// are ignored.
pub fn stream_graph<
    DB: Blockstore,
    T: Borrow<Tipset>,
    ITER: Iterator<Item = T> + Unpin,
    S: CidHashSetLike,
>(
    db: DB,
    tipset_iter: ITER,
    stateroot_limit_exclusive: ChainEpoch,
    seen: S,
) -> ChainStream<DB, ITER, S> {
    stream_chain(db, tipset_iter, stateroot_limit_exclusive, seen).fail_on_dead_links(false)
}

impl<DB: Blockstore, T: Borrow<Tipset>, ITER: Iterator<Item = T> + Unpin, S: CidHashSetLike> Stream
    for ChainStream<DB, ITER, S>
{
    type Item = anyhow::Result<CarBlock>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use Task::*;

        let export_tipset_keys = self.tipset_keys;
        let fail_on_dead_links = self.fail_on_dead_links;
        let stateroot_limit_exclusive = self.stateroot_limit_exclusive;
        let this = self.project();

        // Yield to the runtime every 128 polls to allow cancellation
        {
            *this.n_polled += 1;
            if this.n_polled.is_multiple_of(128) {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        }

        loop {
            while let Some(task) = this.dfs.front_mut() {
                match task {
                    Emit(_, _) => {
                        if let Some(Emit(cid, data)) = this.dfs.pop_front() {
                            if let Some(data) = data {
                                return Poll::Ready(Some(Ok(CarBlock { cid, data })));
                            } else if let Some(data) = this.db.get(&cid)? {
                                return Poll::Ready(Some(Ok(CarBlock {
                                    cid,
                                    data: data.into(),
                                })));
                            } else if fail_on_dead_links {
                                return Poll::Ready(Some(Err(anyhow::anyhow!(
                                    "[Emit] missing key: {cid}"
                                ))));
                            };
                        }
                    }
                    Iterate(epoch, block_cid, _type, cid_vec) => {
                        if *this.track_progress {
                            update_epoch(*epoch);
                        }
                        while let Some(cid) = cid_vec.pop() {
                            // The link traversal implementation assumes there are three types of encoding:
                            // 1. DAG_CBOR: needs to be reachable, so we add it to the queue and load.
                            // 2. IPLD_RAW: WASM blocks, for example. Need to be loaded, but not traversed.
                            // 3. _: ignore all other links
                            // Don't revisit what's already been visited.
                            if should_save_block_to_snapshot(cid) && this.seen.insert(cid)? {
                                if let Some(data) = this.db.get(&cid)? {
                                    if cid.codec() == fvm_ipld_encoding::DAG_CBOR {
                                        let new_values = extract_cids(&data)?;
                                        cid_vec.extend(new_values.into_iter().rev());
                                    }
                                    return Poll::Ready(Some(Ok(CarBlock {
                                        cid,
                                        data: data.into(),
                                    })));
                                } else if fail_on_dead_links {
                                    let type_display = match _type {
                                        IterateType::Message(c) => {
                                            format!("message {c}")
                                        }
                                        IterateType::StateRoot(c) => {
                                            format!("state root {c}")
                                        }
                                        IterateType::MessageReceipts(c) => {
                                            // Forgive message receipts
                                            tracing::trace!(
                                                "[Iterate] missing key: {cid} from message receipts {c} in block {block_cid} at epoch {epoch}"
                                            );
                                            continue;
                                        }
                                        IterateType::EventsRoot(c) => {
                                            // Forgive events
                                            tracing::trace!(
                                                "[Iterate] missing key: {cid} from events root {c} in block {block_cid} at epoch {epoch}"
                                            );
                                            continue;
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
                // Tipset key cid can be convert from and to eth hash, which is useful for Eth APIs
                if export_tipset_keys
                    && let Ok(CarBlock { cid, data }) = tipset.borrow().key().car_block()
                {
                    this.dfs.push_back(Emit(cid, Some(data)));
                }

                for block in tipset.borrow().block_headers() {
                    let (cid, data) = block.car_block()?;
                    if this.seen.insert(cid)? {
                        if *this.track_progress {
                            update_epoch(block.epoch);
                        }
                        // Make sure we always yield a block otherwise.
                        this.dfs.push_back(Emit(cid, Some(data.into())));

                        if block.epoch == 0 {
                            // The genesis block has some kind of dummy parent that needs to be emitted.
                            for p in &block.parents {
                                this.dfs.push_back(Emit(p, None));
                            }
                        }

                        // Process block messages.
                        if block.epoch > stateroot_limit_exclusive {
                            this.dfs.push_back(Iterate(
                                block.epoch,
                                *block.cid(),
                                IterateType::Message(block.messages),
                                DfsIter::from(block.messages)
                                    .filter_map(ipld_to_cid)
                                    .collect(),
                            ));
                            if *this.message_receipts {
                                this.dfs.push_back(Iterate(
                                    block.epoch,
                                    *block.cid(),
                                    IterateType::MessageReceipts(block.message_receipts),
                                    DfsIter::from(block.message_receipts)
                                        .filter_map(ipld_to_cid)
                                        .collect(),
                                ));
                            }
                            // ignore failure as receipts are not required by a lite snapshot
                            if *this.events
                                && let Ok(receipts) =
                                    Receipt::get_receipts(this.db, block.message_receipts)
                            {
                                for receipt in receipts {
                                    if let Some(events_root) = receipt.events_root() {
                                        this.dfs.push_back(Iterate(
                                            block.epoch,
                                            *block.cid(),
                                            IterateType::EventsRoot(events_root),
                                            DfsIter::from(events_root)
                                                .filter_map(ipld_to_cid)
                                                .collect(),
                                        ));
                                    }
                                }
                            }
                        }

                        // Visit the block if it's within required depth. And a special case for `0`
                        // epoch to match Lotus' implementation.
                        if block.epoch == 0 || block.epoch > stateroot_limit_exclusive {
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

pin_project! {
    pub struct IpldStream<DB, S> {
        db: DB,
        cid_vec: Vec<Cid>,
        seen: S,
    }
}

impl<DB, S> IpldStream<DB, S> {
    pub fn new(db: DB, roots: Vec<Cid>, seen: S) -> Self {
        Self {
            db,
            cid_vec: roots,
            seen,
        }
    }
}

impl<DB: Blockstore, S: CidHashSetLike> Stream for IpldStream<DB, S> {
    type Item = anyhow::Result<CarBlock>;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        while let Some(cid) = this.cid_vec.pop() {
            if should_save_block_to_snapshot(cid) && this.seen.insert(cid)? {
                if let Some(data) = this.db.get(&cid)? {
                    if cid.codec() == fvm_ipld_encoding::DAG_CBOR {
                        let new_cids = extract_cids(&data)?;
                        this.cid_vec.extend(new_cids);
                    }
                    return Poll::Ready(Some(Ok(CarBlock {
                        cid,
                        data: data.into(),
                    })));
                } else {
                    return Poll::Ready(Some(Err(anyhow::anyhow!("missing key: {cid}"))));
                }
            }
        }
        // That's it, nothing else to do. End of stream.
        Poll::Ready(None)
    }
}

fn ipld_to_cid(ipld: Ipld) -> Option<Cid> {
    if let Ipld::Link(cid) = ipld {
        Some(cid)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{Chain4U, HeaderBuilder, chain4u};
    use crate::db::MemoryDB;
    use crate::utils::db::CborStoreExt as _;
    use fil_actors_shared::fvm_ipld_amt::Amtv0;
    use futures::TryStreamExt as _;
    use fvm_ipld_encoding::RawBytes;
    use ipld_core::ipld::Ipld;
    use std::sync::Arc;

    #[tokio::test]
    async fn return_data_links_are_not_followed_by_the_walk() -> anyhow::Result<()> {
        let db = Arc::new(MemoryDB::default());

        // A fetchable dag-cbor block, reached only if the walk follows links inside `Return`.
        let embedded = db.put_cbor_default(&Ipld::String("embedded".into()))?;

        // A link in the receipt (positive control): the walk does reach tag-42 links
        // structurally embedded in the receipts AMT.
        let events_root = db.put_cbor_default(&Ipld::String("events-root".into()))?;

        // Build the receipt: `Return` is itself valid dag-cbor encoding a tag-42 link to the
        // `embedded` block.
        let receipt = fvm_shared4::receipt::Receipt {
            exit_code: fvm_shared4::error::ExitCode::OK,
            return_data: RawBytes::new(serde_ipld_dagcbor::to_vec(&Ipld::Link(embedded))?),
            gas_used: 0,
            events_root: Some(events_root),
        };

        let receipts_root = Amtv0::new_from_iter(&db, std::iter::once(receipt))?;

        // One-block tipset whose `message_receipts` points at the AMT. Epoch 1 (> the stateroot
        // limit below) so the receipts branch is reached.
        let c4u = Chain4U::with_blockstore(db.clone());
        chain4u! {
            in c4u;
            [_genesis]
            -> head @ [_header = HeaderBuilder::new().with_message_receipts(receipts_root)]
        };

        let mut stream = stream_chain(&db, std::iter::once(head), 0, CidHashSet::default())
            .with_message_receipts(true)
            // The embedded target is present; other roots (e.g. default state roots) are not, and a
            // missing root must not stall the walk.
            .fail_on_dead_links(false);

        let mut seen = Vec::new();
        while let Some(block) = stream.try_next().await? {
            seen.push(block.cid);
        }

        // Receipts walking is active and reaches the AMT root...
        assert!(
            seen.contains(&receipts_root),
            "receipts AMT root must be reachable with receipts enabled"
        );
        // ...and follows the EventsRoot link inside it.
        assert!(
            seen.contains(&events_root),
            "EventsRoot link inside the receipt must be followed"
        );
        // But the link inside the opaque `Return` byte string is never followed.
        assert!(
            !seen.contains(&embedded),
            "link embedded in Return must not be followed by the walk"
        );

        Ok(())
    }
}

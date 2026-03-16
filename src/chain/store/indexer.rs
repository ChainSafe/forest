// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod ddls;
#[cfg(test)]
mod tests;

use ahash::HashMap;
use anyhow::Context as _;
use cid::Cid;
pub use ddls::{DDLS, PreparedStatements};
use fvm_ipld_blockstore::Blockstore;
use sqlx::Row as _;

use crate::{
    blocks::Tipset,
    chain::{ChainStore, HeadChanges, index::ResolveNullTipset},
    message::{ChainMessage, SignedMessage},
    rpc::{
        chain::types::ChainIndexValidation,
        eth::{eth_tx_from_signed_eth_message, types::EthHash},
    },
    shim::{
        ActorID,
        address::Address,
        clock::{ChainEpoch, EPOCHS_IN_DAY},
        executor::{Receipt, StampedEvent},
    },
    utils::sqlite,
};
use std::{
    ops::DerefMut as _,
    sync::Arc,
    time::{Duration, Instant},
};

type ActorToDelegatedAddressFunc =
    Arc<dyn Fn(ActorID, &Tipset) -> anyhow::Result<Address> + Send + Sync + 'static>;

type RecomputeTipsetStateFunc = Arc<dyn Fn(Tipset) -> anyhow::Result<()> + Send + Sync + 'static>;

type ExecutedMessage = (ChainMessage, Receipt, Option<Vec<StampedEvent>>);

struct IndexedTipsetData {
    pub indexed_messages_count: u64,
    pub indexed_events_count: u64,
    pub indexed_event_entries_count: u64,
}

#[derive(Debug, smart_default::SmartDefault)]
pub struct SqliteIndexerOptions {
    pub gc_retention_epochs: ChainEpoch,
    pub reconcile_empty_index: bool,
    #[default(3 * EPOCHS_IN_DAY as u64)]
    pub max_reconcile_tipsets: u64,
}

impl SqliteIndexerOptions {
    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.gc_retention_epochs == 0 || self.gc_retention_epochs >= EPOCHS_IN_DAY,
            "gc retention epochs must be 0 or greater than {EPOCHS_IN_DAY}"
        );
        Ok(())
    }

    pub fn with_gc_retention_epochs(mut self, gc_retention_epochs: ChainEpoch) -> Self {
        self.gc_retention_epochs = gc_retention_epochs;
        self
    }
}

pub struct SqliteIndexer<BS> {
    options: SqliteIndexerOptions,
    cs: Arc<ChainStore<BS>>,
    db: sqlx::SqlitePool,
    stmts: PreparedStatements,
    actor_to_delegated_address_func: Option<ActorToDelegatedAddressFunc>,
    recompute_tipset_state_func: Option<RecomputeTipsetStateFunc>,
}

impl<BS> SqliteIndexer<BS>
where
    BS: Blockstore,
{
    pub async fn new(
        db: sqlx::SqlitePool,
        cs: Arc<ChainStore<BS>>,
        options: SqliteIndexerOptions,
    ) -> anyhow::Result<Self> {
        options.validate()?;
        sqlite::init_db(
            &db,
            "chain index",
            DDLS.iter().cloned().map(sqlx::query),
            vec![],
        )
        .await?;
        let stmts = PreparedStatements::default();
        Ok(Self {
            options,
            cs,
            db,
            stmts,
            actor_to_delegated_address_func: None,
            recompute_tipset_state_func: None,
        })
    }

    pub fn with_actor_to_delegated_address_func(mut self, f: ActorToDelegatedAddressFunc) -> Self {
        self.actor_to_delegated_address_func = Some(f);
        self
    }

    pub fn with_recompute_tipset_state_func(mut self, f: RecomputeTipsetStateFunc) -> Self {
        self.recompute_tipset_state_func = Some(f);
        self
    }

    pub async fn index_loop(
        &self,
        mut head_change_subscriber: tokio::sync::broadcast::Receiver<HeadChanges>,
    ) -> anyhow::Result<()> {
        loop {
            let HeadChanges { reverts, applies } = head_change_subscriber.recv().await?;
            for ts in reverts {
                if let Err(e) = self.revert_tipset(&ts).await {
                    tracing::warn!(
                        "failed to revert new head@{}({}): {e}",
                        ts.epoch(),
                        ts.key()
                    );
                }
            }
            for ts in applies {
                if let Err(e) = self.index_tipset(&ts).await {
                    tracing::warn!("failed to index new head@{}({}): {e}", ts.epoch(), ts.key());
                }
            }
        }
    }

    pub async fn gc_loop(&self) {
        if self.options.gc_retention_epochs <= 0 {
            tracing::info!("gc retention epochs is not set, skipping gc");
            return;
        }

        let mut ticker = tokio::time::interval(Duration::from_hours(4));
        loop {
            ticker.tick().await;
            self.gc().await;
        }
    }

    async fn gc(&self) {
        tracing::info!("starting index gc");
        let head = self.cs.heaviest_tipset();
        let removal_epoch = head.epoch() - self.options.gc_retention_epochs - 10; // 10 is for some grace period
        if removal_epoch <= 0 {
            tracing::info!("no tipsets to gc");
            return;
        }

        tracing::info!(
            "gc'ing all (reverted and non-reverted) tipsets before epoch {removal_epoch}"
        );
        match sqlx::query(self.stmts.remove_tipsets_before_height)
            .execute(&self.db)
            .await
        {
            Ok(r) => {
                tracing::info!(
                    "gc'd {} entries before epoch {removal_epoch}",
                    r.rows_affected()
                );
            }
            Err(e) => {
                tracing::error!(
                    "failed to remove reverted tipsets before height {removal_epoch}: {e}"
                );
                return;
            }
        }

        // -------------------------------------------------------------------------------------------------
        // Also GC eth hashes

        // Convert `gc_retention_epochs` to number of days
        let gc_retention_days = self.options.gc_retention_epochs / EPOCHS_IN_DAY;
        if gc_retention_days < 1 {
            tracing::info!("skipping gc of eth hashes as retention days is less than 1");
            return;
        }

        tracing::info!("gc'ing eth hashes older than {gc_retention_days} days");
        match sqlx::query(self.stmts.remove_eth_hashes_older_than)
            .execute(&self.db)
            .await
        {
            Ok(r) => {
                tracing::info!(
                    "gc'd {} eth hashes older than {gc_retention_days} days",
                    r.rows_affected()
                );
            }
            Err(e) => {
                tracing::error!("failed to gc eth hashes older than {gc_retention_days} days: {e}");
            }
        }
    }

    pub async fn validate_index(
        &self,
        epoch: ChainEpoch,
        backfill: bool,
    ) -> anyhow::Result<ChainIndexValidation> {
        let head = self.cs.heaviest_tipset();
        anyhow::ensure!(
            epoch < head.epoch(),
            "cannot validate index at epoch {epoch}, can only validate at an epoch less than chain head epoch {}",
            head.epoch()
        );
        let ts =
            self.cs
                .chain_index()
                .tipset_by_height(epoch, head, ResolveNullTipset::TakeOlder)?;
        let is_index_empty: bool = sqlx::query(self.stmts.is_index_empty)
            .fetch_one(&self.db)
            .await?
            .get(0);

        // Canonical chain has a null round at the epoch -> return if index is empty otherwise validate that index also
        // has a null round at this epoch i.e. it does not have anything indexed at all for this epoch
        if ts.epoch() != epoch {
            if is_index_empty {
                return Ok(ChainIndexValidation {
                    height: epoch,
                    is_null_round: true,
                    ..Default::default()
                });
            }
            // validate the db has a hole here and error if not, we don't attempt to repair because something must be very wrong for this to fail
            return self.validate_is_null_round(epoch).await;
        }

        // if the index is empty -> short-circuit and simply backfill if applicable
        if is_index_empty {
            check_backfill_required(epoch, backfill)?;
            return self.backfill_missing_tipset(&ts).await;
        }

        // see if the tipset at this epoch is already indexed or if we need to backfill
        if let Some((reverted_count, non_reverted_count)) =
            self.get_tipset_counts_at_height(epoch).await?
        {
            if reverted_count == 0 && non_reverted_count == 0 {
                check_backfill_required(epoch, backfill)?;
                return self.backfill_missing_tipset(&ts).await;
            } else if reverted_count > 0 && non_reverted_count == 0 {
                anyhow::bail!("index corruption: height {epoch} only has reverted tipsets");
            } else if non_reverted_count > 1 {
                anyhow::bail!("index corruption: height {epoch} has multiple non-reverted tipsets");
            }
        } else {
            check_backfill_required(epoch, backfill)?;
            return self.backfill_missing_tipset(&ts).await;
        }

        // fetch the non-reverted tipset at this epoch
        let indexed_tsk_cid_bytes: Vec<u8> =
            sqlx::query(self.stmts.get_non_reverted_tipset_at_height)
                .bind(epoch)
                .fetch_one(&self.db)
                .await?
                .get(0);
        let indexed_tsk_cid = Cid::read_bytes(indexed_tsk_cid_bytes.as_slice())?;
        let expected_tsk_cid = ts.key().cid()?;
        anyhow::ensure!(
            indexed_tsk_cid == expected_tsk_cid,
            "index corruption: indexed tipset at height {epoch} has key {indexed_tsk_cid}, but canonical chain has {expected_tsk_cid}",
        );
        let (
            IndexedTipsetData {
                indexed_messages_count,
                indexed_events_count,
                indexed_event_entries_count,
            },
            backfilled,
        ) = if let Ok(r) = self.get_and_verify_indexed_data(&ts).await {
            (r, false)
        } else {
            self.backfill_missing_tipset(&ts).await?;
            (self.get_and_verify_indexed_data(&ts).await?, true)
        };
        Ok(ChainIndexValidation {
            tip_set_key: ts.key().clone().into(),
            height: ts.epoch(),
            backfilled,
            indexed_messages_count,
            indexed_events_count,
            indexed_event_entries_count,
            is_null_round: false,
        })
    }

    async fn validate_is_null_round(
        &self,
        epoch: ChainEpoch,
    ) -> anyhow::Result<ChainIndexValidation> {
        // make sure we do not have tipset(reverted or non-reverted) indexed at this epoch
        let is_null_round: bool = sqlx::query(self.stmts.has_null_round_at_height)
            .bind(epoch)
            .fetch_one(&self.db)
            .await?
            .get(0);
        anyhow::ensure!(
            is_null_round,
            "index corruption: height {epoch} should be a null round but is not"
        );
        Ok(ChainIndexValidation {
            height: epoch,
            is_null_round: true,
            ..Default::default()
        })
    }

    async fn backfill_missing_tipset(&self, ts: &Tipset) -> anyhow::Result<ChainIndexValidation> {
        let execution_ts = self.get_next_tipset(ts)?;
        let mut tx = self.db.begin().await?;
        self.index_tipset_and_parent_events_with_tx(&mut tx, &execution_ts)
            .await?;
        tx.commit().await?;
        let IndexedTipsetData {
            indexed_messages_count,
            indexed_events_count,
            indexed_event_entries_count,
        } = self.get_indexed_tipset_data(ts).await?;
        Ok(ChainIndexValidation {
            tip_set_key: ts.key().clone().into(),
            height: ts.epoch(),
            backfilled: true,
            indexed_messages_count,
            indexed_events_count,
            indexed_event_entries_count,
            is_null_round: false,
        })
    }

    fn get_next_tipset(&self, ts: &Tipset) -> anyhow::Result<Tipset> {
        let child = self.cs.chain_index().tipset_by_height(
            ts.epoch() + 1,
            self.cs.heaviest_tipset(),
            ResolveNullTipset::TakeNewer,
        )?;
        anyhow::ensure!(
            child.parents() == ts.key(),
            "chain forked at height {}; please retry your request; err: chain forked",
            ts.epoch()
        );
        Ok(child)
    }

    async fn get_and_verify_indexed_data(&self, ts: &Tipset) -> anyhow::Result<IndexedTipsetData> {
        let indexed_tipset_data = self.get_indexed_tipset_data(ts).await?;
        self.verify_indexed_data(ts, &indexed_tipset_data).await?;
        Ok(indexed_tipset_data)
    }

    /// verifies that the indexed data for a tipset is correct by comparing the number of messages and events
    /// in the chain store to the number of messages and events indexed.
    async fn verify_indexed_data(
        &self,
        ts: &Tipset,
        indexed_tipset_data: &IndexedTipsetData,
    ) -> anyhow::Result<()> {
        let tsk_cid = ts.key().cid()?;
        let tsk_cid_bytes = tsk_cid.to_bytes();
        let execution_ts = self.get_next_tipset(ts)?;

        // given that `ts` is on the canonical chain and `execution_ts` is the next tipset in the chain
        // `ts` can not have reverted events
        let has_reverted_events_in_tipset: bool =
            sqlx::query(self.stmts.has_reverted_events_in_tipset)
                .bind(&tsk_cid_bytes)
                .fetch_one(&self.db)
                .await?
                .get(0);
        anyhow::ensure!(
            !has_reverted_events_in_tipset,
            "index corruption: reverted events found for an executed tipset {tsk_cid} at height {}",
            ts.epoch()
        );
        let executed_messages = self.load_executed_messages(ts, &execution_ts)?;
        anyhow::ensure!(
            executed_messages.len() as u64 == indexed_tipset_data.indexed_messages_count,
            "message count mismatch for height {}: chainstore has {}, index has {}",
            ts.epoch(),
            executed_messages.len(),
            indexed_tipset_data.indexed_messages_count
        );
        let mut events_count = 0;
        let mut event_entries_count = 0;
        for (_, _, events) in &executed_messages {
            if let Some(events) = events {
                events_count += events.len();
                for event in events {
                    event_entries_count += event.event().entries().len();
                }
            }
        }
        anyhow::ensure!(
            events_count as u64 == indexed_tipset_data.indexed_events_count,
            "event count mismatch for height {}: chainstore has {events_count}, index has {}",
            ts.epoch(),
            indexed_tipset_data.indexed_events_count
        );
        anyhow::ensure!(
            event_entries_count as u64 == indexed_tipset_data.indexed_event_entries_count,
            "event entries count mismatch for height {}: chainstore has {event_entries_count}, index has {}",
            ts.epoch(),
            indexed_tipset_data.indexed_event_entries_count
        );

        // compare the events AMT root between the indexed events and the events in the chain state
        for (message, _, _) in executed_messages {}

        Ok(())
    }

    async fn get_indexed_tipset_data(&self, ts: &Tipset) -> anyhow::Result<IndexedTipsetData> {
        let tsk_cid_bytes = ts.key().cid()?.to_bytes();
        let indexed_messages_count = sqlx::query(self.stmts.get_non_reverted_tipset_message_count)
            .bind(&tsk_cid_bytes)
            .fetch_one(&self.db)
            .await?
            .get(0);
        let indexed_events_count = sqlx::query(self.stmts.get_non_reverted_tipset_event_count)
            .bind(&tsk_cid_bytes)
            .fetch_one(&self.db)
            .await?
            .get(0);
        let indexed_event_entries_count =
            sqlx::query(self.stmts.get_non_reverted_tipset_event_entries_count)
                .bind(&tsk_cid_bytes)
                .fetch_one(&self.db)
                .await?
                .get(0);
        Ok(IndexedTipsetData {
            indexed_messages_count,
            indexed_events_count,
            indexed_event_entries_count,
        })
    }

    async fn get_tipset_counts_at_height(
        &self,
        epoch: ChainEpoch,
    ) -> anyhow::Result<Option<(u64, u64)>> {
        let row = sqlx::query(self.stmts.count_tipsets_at_height)
            .bind(epoch)
            .fetch_optional(&self.db)
            .await?;
        Ok(row.map(|r| (r.get(0), r.get(1))))
    }

    fn load_executed_messages(
        &self,
        msg_ts: &Tipset,
        receipt_ts: &Tipset,
    ) -> anyhow::Result<Vec<ExecutedMessage>> {
        let recompute_tipset_state_func = self
            .recompute_tipset_state_func
            .as_ref()
            .context("recompute_tipset_state_func not set")?;
        let msgs = self.cs.messages_for_tipset(msg_ts)?;
        if msgs.is_empty() {
            return Ok(vec![]);
        }
        let mut recomputed = false;
        let recompute = || {
            let tsk_cid = receipt_ts.key().cid()?;
            tracing::warn!(
                "failed to load receipts for tipset {tsk_cid} (epoch {}); recomputing tipset state",
                receipt_ts.epoch()
            );
            recompute_tipset_state_func(msg_ts.clone())?;
            tracing::warn!(
                "successfully recomputed tipset state and loaded events for tipset {tsk_cid} (epoch {})",
                receipt_ts.epoch()
            );
            anyhow::Ok(())
        };
        let receipts = match Receipt::get_receipts(
            self.cs.blockstore(),
            *receipt_ts.parent_message_receipts(),
        ) {
            Ok(receipts) => receipts,
            Err(_) => {
                recompute()?;
                recomputed = true;
                Receipt::get_receipts(self.cs.blockstore(), *receipt_ts.parent_message_receipts())?
            }
        };
        anyhow::ensure!(
            msgs.len() == receipts.len(),
            "mismatching message and receipt counts ({} msgs, {} rcts)",
            msgs.len(),
            receipts.len()
        );
        let mut executed = Vec::with_capacity(msgs.len());
        for (message, receipt) in msgs.into_iter().zip(receipts.into_iter()) {
            let events = if let Some(events_root) = receipt.events_root() {
                Some(
                    match StampedEvent::get_events(self.cs.blockstore(), &events_root) {
                        Ok(events) => events,
                        Err(e) if recomputed => return Err(e),
                        Err(_) => {
                            recompute()?;
                            recomputed = true;
                            StampedEvent::get_events(self.cs.blockstore(), &events_root)?
                        }
                    },
                )
            } else {
                None
            };
            executed.push((message, receipt, events));
        }
        Ok(executed)
    }

    pub async fn populate(&self) -> anyhow::Result<()> {
        let start = Instant::now();
        let head = self.cs.heaviest_tipset();
        tracing::info!(
            "starting to populate chainindex at head epoch {}",
            head.epoch()
        );
        let mut tx = self.db.begin().await?;
        let mut total_indexed = 0;
        for ts in head.chain(self.cs.blockstore()) {
            if let Err(e) = self.index_tipset_with_tx(&mut tx, &ts).await {
                tracing::info!(
                    "stopping chainindex population at epoch {}: {e}",
                    ts.epoch()
                );
                break;
            }
            total_indexed += 1;
        }
        tx.commit().await?;
        tracing::info!(
            "successfully populated chain index with {total_indexed} tipsets, took {}",
            humantime::format_duration(start.elapsed())
        );
        Ok(())
    }

    pub async fn revert_tipset(&self, ts: &Tipset) -> anyhow::Result<()> {
        tracing::debug!("reverting tipset@{}[{}]", ts.epoch(), ts.key().terse());
        let tsk_cid_bytes = ts.key().cid()?.to_bytes();
        // Because of deferred execution in Filecoin, events at tipset T are reverted when a tipset T+1 is reverted.
        // However, the tipet `T` itself is not reverted.
        let pts = Tipset::load_required(self.cs.blockstore(), ts.parents())?;
        let events_tsk_cid_bytes = pts.key().cid()?.to_bytes();
        let mut tx = self.db.begin().await?;
        sqlx::query(self.stmts.update_tipset_to_reverted)
            .bind(&tsk_cid_bytes)
            .execute(tx.deref_mut())
            .await?;
        // events are indexed against the message inclusion tipset, not the message execution tipset.
        // So we need to revert the events for the message inclusion tipset
        sqlx::query(self.stmts.update_events_to_reverted)
            .bind(&events_tsk_cid_bytes)
            .execute(tx.deref_mut())
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn index_tipset(&self, ts: &Tipset) -> anyhow::Result<()> {
        tracing::debug!("indexing tipset@{}[{}]", ts.epoch(), ts.key().terse());
        let mut tx = self.db.begin().await?;
        self.index_tipset_and_parent_events_with_tx(&mut tx, ts)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn index_tipset_with_tx<'a>(
        &self,
        tx: &mut sqlx::SqliteTransaction<'a>,
        ts: &Tipset,
    ) -> anyhow::Result<()> {
        let tsk_cid_bytes = ts.key().cid()?.to_bytes();
        if self
            .restore_tipset_if_exists_with_tx(tx, &tsk_cid_bytes)
            .await?
        {
            Ok(())
        } else {
            let msgs = self
                .cs
                .messages_for_tipset(ts)
                .map_err(|e| anyhow::anyhow!("failed to get messages for tipset: {e}"))?;
            if msgs.is_empty() {
                // If there are no messages, just insert the tipset and return
                sqlx::query(self.stmts.insert_tipset_message)
                    .bind(&tsk_cid_bytes)
                    .bind(ts.epoch())
                    .bind(0)
                    .bind(None::<&[u8]>)
                    .bind(-1)
                    .execute(tx.deref_mut())
                    .await
                    .map_err(|e| anyhow::anyhow!("failed to insert empty tipset: {e}"))?;
            } else {
                for (i, msg) in msgs.into_iter().enumerate() {
                    sqlx::query(self.stmts.insert_tipset_message)
                        .bind(&tsk_cid_bytes)
                        .bind(ts.epoch())
                        .bind(0)
                        .bind(msg.cid().to_bytes())
                        .bind(i as i64)
                        .execute(tx.deref_mut())
                        .await
                        .map_err(|e| anyhow::anyhow!("failed to insert tipset message: {e}"))?;
                }

                for block in ts.block_headers() {
                    let (_, smsgs) = crate::chain::block_messages(self.cs.blockstore(), block)
                        .map_err(|e| anyhow::anyhow!("failed to get messages for block: {e}"))?;
                    for smsg in smsgs.into_iter().filter(SignedMessage::is_delegated) {
                        self.index_signed_message_with_tx(tx, &smsg)
                            .await
                            .map_err(|e| anyhow::anyhow!("failed to index eth tx hash: {e}"))?;
                    }
                }
            }
            Ok(())
        }
    }

    pub async fn index_tipset_and_parent_events_with_tx<'a>(
        &self,
        tx: &mut sqlx::SqliteTransaction<'a>,
        ts: &Tipset,
    ) -> anyhow::Result<()> {
        self.index_tipset_with_tx(tx, ts)
            .await
            .map_err(|e| anyhow::anyhow!("failed to index tipset: {e}"))?;
        if ts.epoch() == 0 {
            // Skip parent if ts is genesis
            return Ok(());
        }
        let pts = Tipset::load_required(self.cs.blockstore(), ts.parents())?;
        // Index the parent tipset if it doesn't exist yet.
        // This is necessary to properly index events produced by executing
        // messages included in the parent tipset by the current tipset (deferred execution).
        self.index_tipset_with_tx(tx, &pts)
            .await
            .map_err(|e| anyhow::anyhow!("failed to index parent tipset: {e}"))?;
        // Now Index events
        self.index_events_with_tx(tx, &pts, ts)
            .await
            .map_err(|e| anyhow::anyhow!("failed to index events: {e}"))
    }

    pub async fn index_events_with_tx<'a>(
        &self,
        tx: &mut sqlx::SqliteTransaction<'a>,
        msg_ts: &Tipset,
        execution_ts: &Tipset,
    ) -> anyhow::Result<()> {
        let actor_to_delegated_address_func = self
            .actor_to_delegated_address_func
            .as_ref()
            .context("indexer can not index events without an address resolver")?;
        // check if we have an event indexed for any message in the `msg_ts` tipset -> if so, there's nothig to do here
        // this makes event inserts idempotent
        let msg_tsk_cid_bytes = msg_ts.key().cid()?.to_bytes();

        // if we've already indexed events for this tipset, mark them as unreverted and return
        let rows_affected = sqlx::query(self.stmts.update_events_to_non_reverted)
            .bind(&msg_tsk_cid_bytes)
            .execute(tx.deref_mut())
            .await
            .map_err(|e| anyhow::anyhow!("failed to unrevert events for tipset: {e}"))?
            .rows_affected();
        if rows_affected > 0 {
            tracing::debug!(
                "unreverted {rows_affected} events for tipset {}",
                msg_ts.key()
            );
            return Ok(());
        }
        let executed_messages = self
            .load_executed_messages(msg_ts, execution_ts)
            .map_err(|e| anyhow::anyhow!("failed to load executed message: {e}"))?;
        let mut event_count = 0;
        let mut address_lookups = HashMap::default();
        for (message, _, events) in executed_messages {
            let msg_cid_bytes = message.cid().to_bytes();

            // read message id for this message cid and tipset key cid
            let message_id: i64 = sqlx::query(self.stmts.get_msg_id_for_msg_cid_and_tipset)
                .bind(&msg_tsk_cid_bytes)
                .bind(&msg_cid_bytes)
                .fetch_optional(tx.deref_mut())
                .await?
                .with_context(|| {
                    format!(
                        "message id not found for message cid {} and tipset key {}",
                        message.cid(),
                        msg_ts.key()
                    )
                })?
                .get(0);

            // Insert events for this message
            if let Some(events) = events {
                for event in events {
                    let emitter = event.emitter();
                    let addr = if let Some(addr) = address_lookups.get(&emitter) {
                        *addr
                    } else {
                        let addr = actor_to_delegated_address_func(emitter, execution_ts)?;
                        address_lookups.insert(emitter, addr);
                        addr
                    };

                    let robust_addr_bytes = if addr.is_delegated() {
                        addr.to_bytes()
                    } else {
                        vec![]
                    };

                    // Insert event into events table
                    let event_id = sqlx::query(self.stmts.insert_event)
                        .bind(message_id)
                        .bind(event_count)
                        .bind(emitter as i64)
                        .bind(robust_addr_bytes)
                        .bind(0)
                        .execute(tx.deref_mut())
                        .await?
                        .last_insert_rowid();

                    for entry in event.event().entries() {
                        let (flags, key, codec, value) = entry.into_parts();
                        sqlx::query(self.stmts.insert_event_entry)
                            .bind(event_id)
                            .bind(is_indexed_flag(flags))
                            .bind([flags as u8].as_slice())
                            .bind(key)
                            .bind(codec as i64)
                            .bind(&value)
                            .execute(tx.deref_mut())
                            .await?;
                    }

                    event_count += 1;
                }
            }
        }
        Ok(())
    }

    pub async fn restore_tipset_if_exists_with_tx<'a>(
        &self,
        tx: &mut sqlx::SqliteTransaction<'a>,
        tsk_cid_bytes: &[u8],
    ) -> anyhow::Result<bool> {
        match sqlx::query(self.stmts.has_tipset)
            .bind(tsk_cid_bytes)
            .fetch_one(tx.deref_mut())
            .await
            .map(|r| r.get(0))
        {
            Ok(exists) => {
                if exists {
                    sqlx::query(self.stmts.update_tipset_to_non_reverted)
                        .bind(tsk_cid_bytes)
                        .execute(tx.deref_mut())
                        .await
                        .map_err(|e| anyhow::anyhow!("failed to restore tipset: {e}"))?;
                }
                Ok(exists)
            }
            Err(e) => anyhow::bail!("failed to check if tipset exists: {e}"),
        }
    }

    pub async fn index_signed_message_with_tx<'a>(
        &self,
        tx: &mut sqlx::SqliteTransaction<'a>,
        smsg: &SignedMessage,
    ) -> anyhow::Result<()> {
        let (_, eth_tx) = eth_tx_from_signed_eth_message(smsg, self.cs.chain_config().eth_chain_id)
            .map_err(|e| anyhow::anyhow!("failed to convert filecoin message to eth tx: {e}"))?;
        let tx_hash = EthHash(
            eth_tx
                .eth_hash()
                .map_err(|e| anyhow::anyhow!("failed to hash transaction: {e}"))?,
        );
        self.index_eth_tx_hash_with_tx(tx, tx_hash, smsg.cid())
            .await
    }

    pub async fn index_eth_tx_hash_with_tx<'a>(
        &self,
        tx: &mut sqlx::SqliteTransaction<'a>,
        tx_hash: EthHash,
        msg_cid: Cid,
    ) -> anyhow::Result<()> {
        _ = sqlx::query(self.stmts.insert_eth_tx_hash)
            .bind(tx_hash.to_string())
            .bind(msg_cid.to_string())
            .execute(tx.deref_mut())
            .await?;
        Ok(())
    }
}

fn is_indexed_flag(flag: u64) -> bool {
    use crate::shim::fvm_shared_latest::event::Flags;
    flag & (Flags::FLAG_INDEXED_KEY.bits() | Flags::FLAG_INDEXED_VALUE.bits()) > 0
}

fn check_backfill_required(epoch: ChainEpoch, backfill: bool) -> anyhow::Result<()> {
    anyhow::ensure!(
        backfill,
        "missing tipset at height {epoch} in the chain index, set backfill flag to true to fix"
    );
    Ok(())
}

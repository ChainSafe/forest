// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Status and life cycle of the chain-export slot shared by user-requested snapshot
//! exports and the automatic snapshot GC.

use crate::shim::clock::ChainEpoch;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::atomic::{self, AtomicI64};
use tokio_util::sync::CancellationToken;

/// What kind of export is (or was last) holding the chain-export slot.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
    strum::Display,
)]
pub enum ChainExportKind {
    /// A snapshot export requested via `Forest.ChainExport`.
    Snapshot,
    /// A diff snapshot export requested via `Forest.ChainExportDiff`.
    DiffSnapshot,
    /// A lite snapshot export performed by the automatic snapshot GC.
    SnapshotGc,
    /// An index backfill requested via `Forest.IndexBackfill`. It holds the same single-flight
    /// slot as exports and the snapshot GC so that these heavy DB operations never overlap.
    IndexBackfill,
}

/// Transitions only through [`ChainExportGuard`]: `Running` while a guard is held, then
/// exactly one terminal state once it drops. `Idle`: no export has run since node start.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
    strum::Display,
)]
pub enum ChainExportState {
    Idle,
    Running,
    Succeeded,
    Cancelled,
    Failed,
}

/// Cold state behind one mutex for consistent reads; only the per-block epoch counters
/// are hot and lock-free.
#[derive(Default)]
struct StatusInner {
    /// `None` while running and before the first export.
    outcome: Option<ChainExportState>,
    kind: Option<ChainExportKind>,
    start_time: Option<DateTime<Utc>>,
    error: Option<String>,
    cancellation_token: Option<CancellationToken>,
    /// See [`ProgressReporter`].
    counters: Arc<ProgressCounters>,
}

impl StatusInner {
    /// A live cancellation token exists exactly while a [`ChainExportGuard`] is held.
    fn is_running(&self) -> bool {
        self.cancellation_token.is_some()
    }
}

#[derive(Default)]
struct ProgressCounters {
    epoch: AtomicI64,
    initial_epoch: AtomicI64,
}

#[derive(Default)]
pub struct ExportStatus {
    inner: parking_lot::Mutex<StatusInner>,
}

/// Read under one lock, so fields are mutually consistent.
pub struct StatusSnapshot {
    pub state: ChainExportState,
    pub kind: Option<ChainExportKind>,
    pub error: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub epoch: ChainEpoch,
    pub initial_epoch: ChainEpoch,
}

pub fn kind_label(kind: Option<ChainExportKind>) -> String {
    kind.map(|k| k.to_string())
        .unwrap_or_else(|| "unknown".into())
}

pub fn format_start_time(start_time: Option<DateTime<Utc>>) -> String {
    start_time
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_else(|| "unknown".into())
}

impl ExportStatus {
    pub fn snapshot(&self) -> StatusSnapshot {
        let inner = self.inner.lock();
        StatusSnapshot {
            state: if inner.is_running() {
                ChainExportState::Running
            } else {
                inner.outcome.unwrap_or(ChainExportState::Idle)
            },
            kind: inner.kind,
            error: inner.error.clone(),
            start_time: inner.start_time,
            epoch: inner.counters.epoch.load(atomic::Ordering::Relaxed),
            initial_epoch: inner.counters.initial_epoch.load(atomic::Ordering::Relaxed),
        }
    }

    /// Check-and-cancel under one lock, so the cancel cannot land on a different export
    /// than the one observed.
    pub fn cancel_running(&self) -> bool {
        if let Some(token) = &self.inner.lock().cancellation_token {
            token.cancel();
            true
        } else {
            false
        }
    }

    /// Holding the mutex makes check-and-start atomic: the lock is the export slot.
    fn try_begin(
        &self,
        kind: ChainExportKind,
        cancellation_token: CancellationToken,
    ) -> anyhow::Result<()> {
        let mut inner = self.inner.lock();
        anyhow::ensure!(
            !inner.is_running(),
            "an active {} export has been running since {}; check `forest-cli snapshot export-status`",
            kind_label(inner.kind),
            format_start_time(inner.start_time),
        );
        *inner = StatusInner {
            outcome: None,
            kind: Some(kind),
            start_time: Some(Utc::now()),
            error: None,
            cancellation_token: Some(cancellation_token),
            counters: Arc::new(ProgressCounters::default()),
        };
        Ok(())
    }

    /// The first terminal outcome recorded for an export wins; later ones are ignored.
    fn record_outcome(&self, outcome: ChainExportState, error: Option<String>) {
        let mut inner = self.inner.lock();
        if inner.outcome.is_none() {
            inner.outcome = Some(outcome);
            inner.error = error;
        }
    }

    fn end(&self) {
        let mut inner = self.inner.lock();
        // Ended without a recorded outcome (panic, or a skipped `finish`): failed.
        if inner.outcome.is_none() {
            inner.outcome = Some(ChainExportState::Failed);
            if !std::thread::panicking() {
                tracing::warn!("chain export guard dropped without a recorded outcome");
            }
        }
        inner.cancellation_token = None;
    }

    pub(super) fn progress_reporter(&self) -> ProgressReporter {
        ProgressReporter(self.inner.lock().counters.clone())
    }
}

/// Bound at creation to its export's freshly allocated counters: a producer task that
/// outlives its export (a Tokio abort lands only at the next await) keeps writing into
/// its own orphaned counters, never the next export's.
#[derive(Clone)]
pub struct ProgressReporter(Arc<ProgressCounters>);

impl ProgressReporter {
    pub fn update_epoch(&self, epoch: ChainEpoch) {
        self.0.epoch.store(epoch, atomic::Ordering::Relaxed);
        _ = self.0.initial_epoch.compare_exchange(
            0,
            epoch,
            atomic::Ordering::Relaxed,
            atomic::Ordering::Relaxed,
        );
    }
}

pub static CHAIN_EXPORT_STATUS: LazyLock<ExportStatus> = LazyLock::new(ExportStatus::default);

#[derive(Debug)]
pub struct ChainExportGuard {
    cancellation_token: CancellationToken,
}

impl ChainExportGuard {
    pub fn try_start_export(kind: ChainExportKind) -> anyhow::Result<Self> {
        let cancellation_token = CancellationToken::new();
        CHAIN_EXPORT_STATUS.try_begin(kind, cancellation_token.clone())?;
        Ok(Self { cancellation_token })
    }

    /// Every export path that holds a [`ChainExportGuard`] must await its work through
    /// this method — an export that does not race against the cancellation token cannot
    /// be cancelled and appears stuck until process restart.
    pub async fn run_cancellable<F: Future>(&self, fut: F) -> Option<F::Output> {
        let output = self.cancellation_token.run_until_cancelled(fut).await;
        if output.is_none() {
            CHAIN_EXPORT_STATUS.record_outcome(ChainExportState::Cancelled, None);
        }
        output
    }

    /// A cancellation observed by [`Self::run_cancellable`] wins over `result`, so
    /// callers need no cancellation special-casing.
    pub fn finish<T>(self, result: anyhow::Result<T>) -> anyhow::Result<T> {
        match &result {
            Ok(_) => CHAIN_EXPORT_STATUS.record_outcome(ChainExportState::Succeeded, None),
            Err(e) => {
                CHAIN_EXPORT_STATUS.record_outcome(ChainExportState::Failed, Some(format!("{e:#}")))
            }
        }
        result
    }
}

impl Drop for ChainExportGuard {
    fn drop(&mut self) {
        // In case some tasks are waiting on this token
        self.cancellation_token.cancel();
        CHAIN_EXPORT_STATUS.end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the invariant documented on [`ChainExportGuard::run_cancellable`].
    #[tokio::test]
    #[serial_test::serial(chain_export)]
    async fn chain_export_cancel_stops_guarded_export() {
        let g = ChainExportGuard::try_start_export(ChainExportKind::Snapshot).unwrap();

        let fut = g.run_cancellable(std::future::pending::<()>());
        // Cancel exactly as the `Forest.ChainExportCancel` handler does.
        assert!(CHAIN_EXPORT_STATUS.cancel_running());
        assert!(
            fut.await.is_none(),
            "cancellation must interrupt the export"
        );

        assert_eq!(
            CHAIN_EXPORT_STATUS.snapshot().state,
            ChainExportState::Running
        );
        drop(g);
        assert_eq!(
            CHAIN_EXPORT_STATUS.snapshot().state,
            ChainExportState::Cancelled
        );
    }

    #[test]
    #[serial_test::serial(chain_export)]
    fn chain_export_status_reports_kind() {
        let g = ChainExportGuard::try_start_export(ChainExportKind::SnapshotGc).unwrap();
        assert_eq!(
            CHAIN_EXPORT_STATUS.snapshot().kind,
            Some(ChainExportKind::SnapshotGc)
        );

        // Rejecting a concurrent export must say what kind of export is in the way.
        let err = ChainExportGuard::try_start_export(ChainExportKind::Snapshot).unwrap_err();
        assert!(
            err.to_string().contains("SnapshotGc"),
            "unexpected error: {err}"
        );

        // The kind outlives the export; the next export replaces it.
        drop(g);
        assert_eq!(
            CHAIN_EXPORT_STATUS.snapshot().kind,
            Some(ChainExportKind::SnapshotGc)
        );
        let _g = ChainExportGuard::try_start_export(ChainExportKind::DiffSnapshot).unwrap();
        assert_eq!(
            CHAIN_EXPORT_STATUS.snapshot().kind,
            Some(ChainExportKind::DiffSnapshot)
        );
    }

    #[test]
    fn chain_export_state_starts_idle() {
        assert_eq!(
            ExportStatus::default().snapshot().state,
            ChainExportState::Idle
        );
    }

    /// Pins the transitions documented on [`ChainExportState`].
    #[tokio::test]
    #[serial_test::serial(chain_export)]
    async fn chain_export_state_machine() {
        let g = ChainExportGuard::try_start_export(ChainExportKind::Snapshot).unwrap();
        assert_eq!(
            CHAIN_EXPORT_STATUS.snapshot().state,
            ChainExportState::Running
        );
        g.finish(anyhow::Ok(())).unwrap();
        assert_eq!(
            CHAIN_EXPORT_STATUS.snapshot().state,
            ChainExportState::Succeeded
        );

        // Failure.
        let g = ChainExportGuard::try_start_export(ChainExportKind::Snapshot).unwrap();
        g.finish(anyhow::Result::<()>::Err(anyhow::anyhow!(
            "missing state root"
        )))
        .unwrap_err();
        assert_eq!(
            CHAIN_EXPORT_STATUS.snapshot().state,
            ChainExportState::Failed
        );
        assert_eq!(
            CHAIN_EXPORT_STATUS.snapshot().error.as_deref(),
            Some("missing state root")
        );

        // A guard dropped without `finish` still lands in `Failed`; the previous
        // failure's error does not leak into the new export.
        let g = ChainExportGuard::try_start_export(ChainExportKind::Snapshot).unwrap();
        drop(g);
        assert_eq!(
            CHAIN_EXPORT_STATUS.snapshot().state,
            ChainExportState::Failed
        );
        assert_eq!(CHAIN_EXPORT_STATUS.snapshot().error, None);

        // A cancelled export whose body then bails: no error recorded.
        let g = ChainExportGuard::try_start_export(ChainExportKind::SnapshotGc).unwrap();
        assert!(CHAIN_EXPORT_STATUS.cancel_running());
        assert!(
            g.run_cancellable(std::future::pending::<()>())
                .await
                .is_none()
        );
        g.finish(anyhow::Result::<()>::Err(anyhow::anyhow!(
            "snapshot GC export was cancelled"
        )))
        .unwrap_err();
        assert_eq!(
            CHAIN_EXPORT_STATUS.snapshot().state,
            ChainExportState::Cancelled
        );
        assert_eq!(CHAIN_EXPORT_STATUS.snapshot().error, None);

        // A cancel after completion must not flip the terminal state.
        let g = ChainExportGuard::try_start_export(ChainExportKind::Snapshot).unwrap();
        g.finish(anyhow::Ok(())).unwrap();
        assert!(!CHAIN_EXPORT_STATUS.cancel_running());
        assert_eq!(
            CHAIN_EXPORT_STATUS.snapshot().state,
            ChainExportState::Succeeded
        );
    }
}

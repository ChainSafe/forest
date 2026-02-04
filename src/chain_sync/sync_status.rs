// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::blocks::TipsetKey;
use crate::lotus_json::lotus_json_with_self;
use crate::networks::calculate_expected_epoch;
use crate::shim::clock::ChainEpoch;
use crate::state_manager::StateManager;
use chrono::{DateTime, Utc};
use fvm_ipld_blockstore::Blockstore;
use parking_lot::RwLock;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::log;

// Node considered synced if the head is within this threshold.
const SYNCED_EPOCH_THRESHOLD: u64 = 10;

/// Represents the overall synchronization status of the Forest node.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    JsonSchema,
    strum::Display,
    strum::EnumString,
)]
pub enum NodeSyncStatus {
    /// Node is initializing, status not yet determined.
    #[default]
    #[strum(to_string = "Intializing")]
    Initializing,
    /// Node is significantly behind the network head and actively downloading/validating.
    #[strum(to_string = "Syncing")]
    Syncing,
    /// Node is close to the network head, within the `SYNCED_EPOCH_THRESHOLD`.
    #[strum(to_string = "Synced")]
    Synced,
    /// An error occurred during the sync process.
    #[strum(to_string = "Error")]
    Error,
    /// Node is configured to not sync (offline mode).
    #[strum(to_string = "Offline")]
    Offline,
}

/// Represents the stage of processing for a specific chain fork being tracked.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
    JsonSchema,
    strum::Display,
    strum::EnumString,
)]
pub enum ForkSyncStage {
    /// Fetching necessary block headers for this fork.
    #[strum(to_string = "Fetching Headers")]
    FetchingHeaders,
    /// Validating tipsets and messages for this fork.
    #[strum(to_string = "Validating Tipsets")]
    ValidatingTipsets,
    /// This fork sync process is complete (e.g., reached target, merged, or deemed invalid).
    #[strum(to_string = "Complete")]
    Complete,
    /// Progress is stalled, potentially waiting for dependencies.
    #[strum(to_string = "Stalled")]
    Stalled,
    /// An error occurred processing this specific fork.
    #[strum(to_string = "Error")]
    Error,
}

/// Contains information about a specific chain/fork the node is actively tracking or syncing.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
pub struct ForkSyncInfo {
    /// The target tipset key for this synchronization task.
    #[schemars(with = "crate::lotus_json::LotusJson<TipsetKey>")]
    #[serde(with = "crate::lotus_json")]
    pub(crate) target_tipset_key: TipsetKey,
    /// The target epoch for this synchronization task.
    pub(crate) target_epoch: ChainEpoch,
    /// The lowest epoch that still needs processing (fetching or validating) for this target.
    /// This helps indicate the start of the current sync range.
    pub(crate) target_sync_epoch_start: ChainEpoch,
    /// The current stage of processing for this fork.
    pub(crate) stage: ForkSyncStage,
    /// The epoch of the heaviest fully validated tipset on the node's main chain.
    /// This shows overall node progress, distinct from fork-specific progress.
    pub(crate) validated_chain_head_epoch: ChainEpoch,
    /// When processing for this fork started.
    pub(crate) start_time: Option<DateTime<Utc>>,
    /// Last time status for this fork was updated.
    pub(crate) last_updated: Option<DateTime<Utc>>,
}

pub type SyncStatus = Arc<RwLock<SyncStatusReport>>;

/// Contains information about the current status of the node's synchronization process.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, JsonSchema)]
pub struct SyncStatusReport {
    /// Overall status of the node's synchronization.
    pub(crate) status: NodeSyncStatus,
    /// The epoch of the heaviest validated tipset on the node's main chain.
    pub(crate) current_head_epoch: ChainEpoch,
    /// The tipset key of the current heaviest validated tipset.
    #[schemars(with = "crate::lotus_json::LotusJson<TipsetKey>")]
    #[serde(with = "crate::lotus_json")]
    pub(crate) current_head_key: Option<TipsetKey>,
    // Current highest epoch on the network.
    pub(crate) network_head_epoch: ChainEpoch,
    /// Estimated number of epochs the node is behind the network head.
    /// Can be negative if the node is slightly ahead, due to estimation variance.
    pub(crate) epochs_behind: i64,
    /// List of active fork synchronization tasks the node is currently handling.
    pub(crate) active_forks: Vec<ForkSyncInfo>,
    /// When the node process started.
    pub(crate) node_start_time: DateTime<Utc>,
    /// Last time this status report was generated.
    pub(crate) last_updated: DateTime<Utc>,
}

lotus_json_with_self!(SyncStatusReport);

impl SyncStatusReport {
    pub(crate) fn init() -> Self {
        Self {
            node_start_time: Utc::now(),
            ..Default::default()
        }
    }

    /// Updates the sync status report based on the current state of the node and network.
    /// This does not modify the existing report but returns a new one with updated information.
    pub(crate) fn update<DB: Blockstore + Sync + Send + 'static>(
        &self,
        state_manager: &StateManager<DB>,
        active_forks: Vec<ForkSyncInfo>,
        stateless_mode: bool,
    ) -> Self {
        let heaviest = state_manager.chain_store().heaviest_tipset();
        let current_head_epoch = heaviest.epoch();
        let current_head_key = Some(heaviest.key().clone());

        let last_updated = Utc::now();
        let last_updated_ts = last_updated.timestamp() as u64;
        let seconds_per_epoch = state_manager.chain_config().block_delay_secs;
        let network_head_epoch = calculate_expected_epoch(
            last_updated_ts,
            state_manager.chain_store().genesis_block_header().timestamp,
            seconds_per_epoch,
        );

        let epochs_behind = network_head_epoch.saturating_sub(current_head_epoch);
        log::trace!(
            "Sync status report: current head epoch: {}, network head epoch: {}, epochs behind: {}",
            current_head_epoch,
            network_head_epoch,
            epochs_behind
        );

        let time_diff = last_updated_ts.saturating_sub(heaviest.min_timestamp());
        let status = match stateless_mode {
            true => NodeSyncStatus::Offline,
            false => {
                if time_diff < seconds_per_epoch as u64 * SYNCED_EPOCH_THRESHOLD {
                    NodeSyncStatus::Synced
                } else {
                    NodeSyncStatus::Syncing
                }
            }
        };

        Self {
            node_start_time: self.node_start_time,
            current_head_epoch,
            current_head_key,
            network_head_epoch,
            epochs_behind,
            status,
            active_forks,
            last_updated,
        }
    }

    pub(crate) fn is_synced(&self) -> bool {
        self.status == NodeSyncStatus::Synced
    }

    pub(crate) fn get_min_starting_block(&self) -> Option<ChainEpoch> {
        self.active_forks
            .iter()
            .map(|fork_info| fork_info.target_sync_epoch_start)
            .min()
    }
}

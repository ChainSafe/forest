// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::blocks::TipsetKey;
use crate::lotus_json::lotus_json_with_self;
use crate::networks::calculate_expected_epoch;
use crate::shim::clock::ChainEpoch;
use crate::state_manager::StateManager;
use chrono::{DateTime, Utc};
use fvm_ipld_blockstore::Blockstore;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::Formatter;
use std::sync::Arc;

/// Represents the overall synchronization status of the Forest node.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema)]
pub enum NodeSyncStatus {
    /// Node is initializing, status not yet determined.
    #[default]
    Initializing,
    /// Node is significantly behind the network head and actively downloading/validating.
    Syncing,
    /// Node is close to the network head (e.g., within a configurable threshold like ~5 epochs).
    Synced,
    /// An error occurred during the sync process.
    Error(String),
    /// Node is configured to not sync (offline mode).
    Offline,
}

/// Represents the stage of processing for a specific chain fork being tracked.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
pub enum ForkSyncStage {
    /// Fetching necessary block headers for this fork.
    FetchingHeaders,
    /// Validating tipsets and messages for this fork.
    ValidatingTipsets,
    /// This fork sync process is complete (e.g., reached target, merged, or deemed invalid).
    Complete,
    /// Progress is stalled, potentially waiting for dependencies.
    Stalled,
    /// An error occurred processing this specific fork.
    Error(String),
}

impl std::fmt::Display for ForkSyncStage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ForkSyncStage::FetchingHeaders => write!(f, "Fetching Headers"),
            ForkSyncStage::ValidatingTipsets => write!(f, "Validating Tipsets"),
            ForkSyncStage::Complete => write!(f, "Complete"),
            ForkSyncStage::Stalled => write!(f, "Stalled"),
            ForkSyncStage::Error(e) => write!(f, "{}", format!("Error: {}", e)),
        }
    }
}

/// Contains information about a specific chain/fork the node is actively tracking or syncing.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
pub struct ForkSyncInfo {
    /// The target tipset key for this synchronization task.
    #[schemars(with = "crate::lotus_json::LotusJson<TipsetKey>")] // Keep LotusJson for TipsetKey if needed
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

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, JsonSchema)]
pub struct ForestSyncStatusReport {
    /// Overall status of the node's synchronization.
    pub status: NodeSyncStatus,
    /// The epoch of the heaviest validated tipset on the node's main chain.
    pub current_head_epoch: ChainEpoch,
    /// The tipset key of the current heaviest validated tipset.
    #[schemars(with = "crate::lotus_json::LotusJson<TipsetKey>")]
    #[serde(with = "crate::lotus_json")]
    pub current_head_key: Option<TipsetKey>,
    /// An estimation of the current highest epoch on the network.
    pub network_head_epoch: ChainEpoch,
    /// Estimated number of epochs the node is behind the network head.
    /// Can be negative if the node is slightly ahead, due to estimation variance.
    pub epochs_behind: i64,
    /// List of active fork synchronization tasks the node is currently handling.
    pub active_forks: Vec<ForkSyncInfo>,
    /// When the node process started.
    pub node_start_time: DateTime<Utc>,
    /// Last time this status report was generated.
    pub last_updated: DateTime<Utc>,
}

lotus_json_with_self!(ForestSyncStatusReport);

impl ForestSyncStatusReport {
    pub(crate) fn new() -> Self {
        Self {
            node_start_time: Utc::now(),
            ..Default::default()
        }
    }

    pub(crate) fn set_current_chain_head(&mut self, tipset_key: TipsetKey, epoch: ChainEpoch) {
        self.current_head_key = Some(tipset_key);
        self.current_head_epoch = epoch;
    }

    pub(crate) fn set_network_head(&mut self, epoch: ChainEpoch) {
        self.network_head_epoch = epoch;
    }

    pub(crate) fn set_epochs_behind(&mut self, epochs_behind: i64) {
        self.epochs_behind = epochs_behind;
    }

    pub(crate) fn set_status(&mut self, status: NodeSyncStatus) {
        self.status = status;
    }

    pub(crate) fn set_active_forks(&mut self, active_forks: Vec<ForkSyncInfo>) {
        self.active_forks = active_forks;
    }

    pub(crate) fn update<DB: Blockstore + Sync + Send + 'static>(
        &mut self,
        state_manager: &Arc<StateManager<DB>>,
        current_active_forks: Vec<ForkSyncInfo>,
        stateless_mode: bool,
    ) {
        let heaviest = state_manager.chain_store().heaviest_tipset();
        let current_chain_head_epoch = heaviest.epoch();
        self.set_current_chain_head(heaviest.key().clone(), current_chain_head_epoch);
        let network_head_epoch = calculate_expected_epoch(
            Utc::now().timestamp() as u64,
            state_manager.chain_store().genesis_block_header().timestamp,
            state_manager.chain_config().block_delay_secs,
        );

        self.set_network_head(network_head_epoch.clone() as ChainEpoch);
        self.set_epochs_behind(network_head_epoch as i64 - current_chain_head_epoch as i64);
        let seconds_per_epoch = state_manager.chain_config().block_delay_secs;
        let time_diff = (Utc::now().timestamp() as u64).saturating_sub(heaviest.min_timestamp());

        match stateless_mode {
            true => self.set_status(NodeSyncStatus::Offline),
            false => {
                if time_diff < seconds_per_epoch as u64 * 5 {
                    self.set_status(NodeSyncStatus::Synced)
                } else {
                    self.set_status(NodeSyncStatus::Syncing)
                }
            }
        }
        self.set_active_forks(current_active_forks);
        self.last_updated = Utc::now();
    }
}

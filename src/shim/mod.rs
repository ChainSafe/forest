// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod address;
pub mod bigint;
pub mod clock;
pub mod crypto;
pub mod deal;
pub mod econ;
pub mod error;
pub mod executor;
pub mod externs;
pub mod gas;
pub mod machine;
pub mod message;
pub mod piece;
pub mod randomness;
pub mod sector;
pub mod state_tree;
pub mod version;

/// Several operations in the FVM can be "traced", e.g
/// - [`crate::interpreter::VM::apply_block_messages`]
///
/// This involves _logical tracing_, where e.g message executions are accumulated
/// in-code, for displaying to the user e.g [`crate::cli::subcommands::snapshot_cmd::SnapshotCommands::ComputeState`]
///
/// This enum indicates whether trace should be accumulated or not.
///
/// # API Hazard
/// This needs careful redesign: <https://github.com/ChainSafe/forest/issues/3405>
#[derive(Default, Clone, Copy)]
pub enum TraceAction {
    /// Collect trace for the givven operation
    Accumulate,
    /// Do not collect trace
    #[default]
    Ignore,
}

impl TraceAction {
    /// Perform `f` if tracing should happen.
    pub fn then<T>(&self, f: impl FnOnce() -> T) -> Option<T> {
        match self {
            TraceAction::Accumulate => Some(f()),
            TraceAction::Ignore => None,
        }
    }
    /// Should tracing be collected?
    pub fn is_accumulate(&self) -> bool {
        matches!(self, TraceAction::Accumulate)
    }
}

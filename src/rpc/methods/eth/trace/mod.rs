// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Ethereum trace construction and state diff logic.
//!
//! Submodules:
//! - [`parity`] — builds Parity-compatible [`types::EthTrace`] entries from
//!   FVM execution traces.
//! - [`geth`] - builds Geth-compatible entries from the FVM execution traces.
//! - [`state_diff`] — computes account-level state diffs between pre/post
//!   execution.
//! - [`types`] — shared type definitions for all `trace_*` RPC responses.

mod geth;
mod parity;
mod state_diff;
#[cfg(test)]
mod test_helpers;
pub(crate) mod types;
mod utils;

pub(super) use geth::*;
pub(super) use parity::*;
pub(crate) use state_diff::build_state_diff;

use super::lookup_eth_address;
use super::types::EthAddress;
use crate::{
    prelude::*,
    shim::{address::Address, state_tree::StateTree},
};
use types::EthTrace;

/// Shared mutable context threaded through recursive trace building.
///
/// Used by both Parity-style and Geth-style trace constructors to track
/// the current caller, collected traces, and subtrace count.
#[derive(Default)]
pub(super) struct Environment {
    pub(super) caller: EthAddress,
    pub(super) is_evm: bool,
    pub(super) subtrace_count: i64,
    pub(super) traces: Vec<EthTrace>,
    pub(super) last_byte_code: Option<EthAddress>,
}

pub(super) fn base_environment<BS: Blockstore + ShallowClone + Send + Sync>(
    state: &StateTree<BS>,
    from: &Address,
) -> anyhow::Result<Environment> {
    let sender = lookup_eth_address(from, state)?
        .with_context(|| format!("top-level message sender {from} could not be found"))?;
    Ok(Environment {
        caller: sender,
        ..Environment::default()
    })
}

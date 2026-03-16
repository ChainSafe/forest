// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Ethereum trace construction and state diff logic.
//!
//! Submodules:
//! - [`parity`] — builds Parity-compatible [`types::EthTrace`] entries from
//!   FVM execution traces.
//! - [`state_diff`] — computes account-level state diffs between pre/post
//!   execution.
//! - [`types`] — shared type definitions for all `trace_*` RPC responses.

mod parity;
mod state_diff;
pub(crate) mod types;

pub(super) use parity::*;
pub(crate) use state_diff::build_state_diff;

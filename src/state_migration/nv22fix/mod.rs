// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the fix logic for the `NV22` calibration network fix.
//! The corresponding Go implementation can be found here:
//! <https://github.com/filecoin-project/lotus/pull/11776>.
mod migration;

/// Run migration for `NV22fix`. This should be the only exported method in this
/// module.
pub use migration::run_migration;

use crate::{define_system_states, impl_system, impl_verifier};

define_system_states!(
    fil_actor_system_state::v13::State,
    fil_actor_system_state::v13::State
);

impl_system!();
impl_verifier!();

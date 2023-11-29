// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV21` calibration network fix.

mod migration;

/// Run migration for `NV21` calibration network fix. This should be the only exported method in this
/// module.
pub use migration::run_migration;

use crate::{define_system_states, impl_system, impl_verifier};

define_system_states!(
    fil_actor_system_state::v12::State,
    fil_actor_system_state::v12::State
);

impl_system!();
impl_verifier!();

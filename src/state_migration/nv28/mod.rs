// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV28` upgrade.
mod migration;

/// Run migration for `NV28`. This should be the only exported method in this
/// module.
#[allow(unused)]
pub use migration::run_migration;

use crate::{define_system_states, impl_system, impl_verifier};

define_system_states!(
    fil_actor_system_state::v17::State,
    fil_actor_system_state::v18::State
);

impl_system!();
impl_verifier!();

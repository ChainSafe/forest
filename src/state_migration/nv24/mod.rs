// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV24` upgrade.
mod migration;

/// Run migration for `NV24`. This should be the only exported method in this
/// module.
#[allow(unused_imports)]
pub use migration::run_migration;

use crate::{define_system_states, impl_system, impl_verifier};

define_system_states!(
    fil_actor_system_state::v14::State,
    // TODO(forest): https://github.com/ChainSafe/forest/issues/4804
    // This should point to the new state type, e.g., `fil_actor_system_state::v15::State`
    fil_actor_system_state::v14::State
);

impl_system!();
impl_verifier!();

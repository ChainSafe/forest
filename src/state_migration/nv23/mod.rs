// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV23` upgrade.
//! The corresponding Go implementation can be found here:
//! <https://github.com/filecoin-project/go-state-types/tree/65098120e3d0b5136015fa5d1c50dba47abe0c69/builtin/v14/migration>

mod migration;
mod mining_reserve;

/// Run migration for `NV23`. This should be the only exported method in this
/// module.
pub use migration::run_migration;

use crate::{define_system_states, impl_system, impl_verifier};

define_system_states!(
    fil_actor_system_state::v13::State,
    fil_actor_system_state::v14::State
);

impl_system!();
impl_verifier!();

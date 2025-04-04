// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the migration logic for the `NV26 fix` that happened on calibration
//! network after the `NV26` upgrade (but before it landed on mainnet). Read more details on the
//! issue [here](https://github.com/filecoin-project/community/discussions/74#discussioncomment-12720764).
mod migration;

/// Run migration for `NV26fix`. This should be the only exported method in this
/// module.
pub use migration::run_migration;

use crate::{define_system_states, impl_system, impl_verifier};

define_system_states!(
    fil_actor_system_state::v16::State,
    fil_actor_system_state::v16::State
);

impl_system!();
impl_verifier!();

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Re-export of actors functionality with v2 feature flag set.
//!
//! The reason for this crate is the restriction from having a dependency referenced twice,
//! where we actually do want to reference a lot of duplicate logic for actors versions.
//!
//! If only one copy of actors is needed, the actors crate should be used with the `v2` feature
//! set instead.

pub use actor::*;

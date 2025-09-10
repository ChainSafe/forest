// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

/// Creates state decode params tests for the System actor.
pub fn create_tests(tipset: &Tipset) -> Result<Vec<RpcTest>> {
    Ok(vec![RpcTest::identity(StateDecodeParams::request((
        Address::SYSTEM_ACTOR,
        fil_actor_system_state::v16::Method::Constructor as u64,
        vec![],
        tipset.key().into(),
    ))?)])
}

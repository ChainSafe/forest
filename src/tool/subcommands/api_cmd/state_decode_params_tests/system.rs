// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use fil_actor_system_state::v17::*;

/// Creates state decode params tests for the System actor.
pub fn create_tests(tipset: &Tipset) -> Result<Vec<RpcTest>> {
    Ok(vec![RpcTest::identity(StateDecodeParams::request((
        Address::SYSTEM_ACTOR,
        Method::Constructor as u64,
        vec![],
        tipset.key().into(),
    ))?)])
}

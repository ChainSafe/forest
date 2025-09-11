// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use fil_actor_ethaccount_state::v17::*;

/// Creates state decode params tests for the EthAccount actor.
pub fn create_tests(tipset: &Tipset) -> anyhow::Result<Vec<RpcTest>> {
    Ok(vec![RpcTest::identity(StateDecodeParams::request((
        Address::new_delegated(Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR.id()?, &[0; 20]).unwrap(),
        Method::Constructor as u64,
        vec![],
        tipset.key().into(),
    ))?)])
}

// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use fil_actor_cron_state::v17::*;

/// Creates state decode params tests for the Cron actor.
pub fn create_tests(tipset: &Tipset) -> anyhow::Result<Vec<RpcTest>> {
    // let cron_constructor_params = ConstructorParams {
    //     entries: vec![Entry {
    //         receiver: Address::new_id(1000).into(),
    //         method_num: EpochTick as u64,
    //     }],
    // };

    Ok(vec![
        // // TODO(go-state-types): https://github.com/filecoin-project/go-state-types/issues/396
        // Enable this test when lotus supports it in go-state-types.
        // RpcTest::identity(StateDecodeParams::request((
        //     Address::CRON_ACTOR,
        //     Constructor as u64,
        //     to_vec(&cron_constructor_params)?,
        //     tipset.key().into(),
        // ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::CRON_ACTOR,
            Method::EpochTick as u64,
            vec![],
            tipset.key().into(),
        ))?),
    ])
}

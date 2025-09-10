// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

/// Creates state decode params tests for the Cron actor.
pub fn create_tests(tipset: &Tipset) -> anyhow::Result<Vec<RpcTest>> {
    // let cron_constructor_params = fil_actor_cron_state::v16::ConstructorParams {
    //     entries: vec![fil_actor_cron_state::v16::Entry {
    //         receiver: Address::new_id(1000).into(),
    //         method_num: fil_actor_cron_state::v16::Method::EpochTick as u64,
    //     }],
    // };

    Ok(vec![
        // // TODO(go-state-types): https://github.com/filecoin-project/go-state-types/issues/396
        // Enable this test when lotus supports it in go-state-types.
        // RpcTest::identity(StateDecodeParams::request((
        //     Address::CRON_ACTOR,
        //     fil_actor_cron_state::v16::Method::Constructor as u64,
        //     to_vec(&cron_constructor_params)?,
        //     tipset.key().into(),
        // ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::CRON_ACTOR,
            fil_actor_cron_state::v16::Method::EpochTick as u64,
            vec![],
            tipset.key().into(),
        ))?),
    ])
}

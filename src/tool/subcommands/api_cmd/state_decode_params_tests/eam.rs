// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

/// Creates state decode params tests for the EAM actor.
pub fn create_tests(tipset: &Tipset) -> Result<Vec<RpcTest>> {
    let create_params = fil_actor_eam_state::v16::CreateParams {
        initcode: vec![0x11, 0x22, 0x33, 0x44, 0x55], // dummy data
        nonce: 2,
    };

    let create_params2 = fil_actor_eam_state::v16::Create2Params {
        initcode: vec![0x11, 0x22, 0x33, 0x44, 0x55], // dummy data
        salt: [0; 32],
    };

    let create_external_params =
        fil_actor_eam_state::v16::CreateExternalParams(vec![0x11, 0x22, 0x33, 0x44, 0x55]);

    Ok(vec![
        RpcTest::identity(StateDecodeParams::request((
            Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR,
            fil_actor_eam_state::v16::Method::Constructor as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR,
            fil_actor_eam_state::v16::Method::Create as u64,
            to_vec(&create_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR,
            fil_actor_eam_state::v16::Method::Create2 as u64,
            to_vec(&create_params2)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR,
            fil_actor_eam_state::v16::Method::CreateExternal as u64,
            to_vec(&create_external_params)?,
            tipset.key().into(),
        ))?),
    ])
}

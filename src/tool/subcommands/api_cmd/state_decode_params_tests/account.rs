// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

/// Creates state decode params tests for the Account actor.
pub fn create_tests(tipset: &Tipset) -> Result<Vec<RpcTest>> {
    let account_constructor_params = fil_actor_account_state::v16::types::ConstructorParams {
        address: Address::new_id(1234).into(),
    };

    let account_auth_params = fil_actor_account_state::v16::types::AuthenticateMessageParams {
        signature: vec![0x00; 32], // dummy signature
        message: b"test message".to_vec(),
    };

    const ACCOUNT_ADDRESS: Address = Address::new_id(1234);
    Ok(vec![
        RpcTest::identity(StateDecodeParams::request((
            ACCOUNT_ADDRESS,
            1,
            to_vec(&account_constructor_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            ACCOUNT_ADDRESS,
            2643134072, // frc42_dispatch::method_hash!("AuthenticateMessage"),
            to_vec(&account_auth_params)?,
            tipset.key().into(),
        ))?),
    ])
}

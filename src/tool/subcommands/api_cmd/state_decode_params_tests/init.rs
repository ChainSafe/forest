// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

/// Creates state decode params tests for the Init actor.
pub fn create_tests(tipset: &Tipset) -> Result<Vec<RpcTest>> {
    let init_constructor_params = fil_actor_init_state::v16::ConstructorParams {
        network_name: "calibnet".to_string(),
    };

    let init_exec4_params = fil_actor_init_state::v16::Exec4Params {
        code_cid: Cid::default(),
        constructor_params: fvm_ipld_encoding::RawBytes::new(vec![0x12, 0x34, 0x56]), // dummy bytecode
        subaddress: fvm_ipld_encoding::RawBytes::new(vec![0x12, 0x34, 0x56]), // dummy bytecode
    };

    let init_exec_params = fil_actor_init_state::v16::ExecParams {
        code_cid: Cid::default(),
        constructor_params: fvm_ipld_encoding::RawBytes::new(vec![0x12, 0x34, 0x56]), // dummy bytecode
    };

    use fil_actor_init_state::v16::Method;
    Ok(vec![
        RpcTest::identity(StateDecodeParams::request((
            Address::INIT_ACTOR,
            Method::Constructor as u64,
            to_vec(&init_constructor_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::INIT_ACTOR,
            Method::Exec as u64,
            to_vec(&init_exec_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::INIT_ACTOR,
            Method::Exec4 as u64,
            to_vec(&init_exec4_params)?,
            tipset.key().into(),
        ))?),
    ])
}

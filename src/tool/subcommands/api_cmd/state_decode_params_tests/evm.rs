// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::rpc::eth::types::GetStorageAtParams;
use fil_actor_evm_state::evm_shared::v17::address::EthAddress;
use fil_actor_evm_state::v17::*;
use std::str::FromStr;

const EVM_ADDRESS: &str = "t410fbqoynu2oi2lxam43knqt6ordiowm2ywlml27z4i";

/// Creates state decode params tests for the EVM actor.
pub fn create_tests(tipset: &Tipset) -> Result<Vec<RpcTest>> {
    let evm_constructor_params = ConstructorParams {
        creator: EthAddress([0; 20]),
        initcode: fvm_ipld_encoding::RawBytes::new(vec![0x12, 0x34, 0x56]), // dummy bytecode
    };

    let evm_invoke_contract_params = InvokeContractParams {
        input_data: vec![0x11, 0x22, 0x33, 0x44, 0x55], // dummy input data
    };

    let evm_delegate_call_params = DelegateCallParams {
        code: Cid::default(),
        input: vec![0x11, 0x22, 0x33, 0x44, 0x55], // dummy input data
        caller: EthAddress([0; 20]),
        value: TokenAmount::default().into(),
    };

    let evm_get_storage_at_params = GetStorageAtParams::new(vec![0xa])?;

    Ok(vec![
        RpcTest::identity(StateDecodeParams::request((
            Address::from_str(EVM_ADDRESS)?,
            Method::Constructor as u64,
            to_vec(&evm_constructor_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::from_str(EVM_ADDRESS)?,
            Method::Resurrect as u64,
            to_vec(&evm_constructor_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::from_str(EVM_ADDRESS)?,
            Method::GetBytecode as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::from_str(EVM_ADDRESS)?,
            Method::GetBytecodeHash as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::from_str(EVM_ADDRESS)?,
            Method::InvokeContract as u64,
            to_vec(&evm_invoke_contract_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::from_str(EVM_ADDRESS)?,
            Method::InvokeContractDelegate as u64,
            to_vec(&evm_delegate_call_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::from_str(EVM_ADDRESS)?,
            Method::GetStorageAt as u64,
            evm_get_storage_at_params.serialize_params()?,
            tipset.key().into(),
        ))?),
    ])
}

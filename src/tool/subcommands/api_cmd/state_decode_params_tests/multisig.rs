// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use base64::{Engine, prelude::BASE64_STANDARD};

/// Creates state decode params tests for the Multisig actor.
pub fn create_tests(tipset: &Tipset) -> Result<Vec<RpcTest>> {
    let multisig_constructor_params = fil_actor_multisig_state::v16::ConstructorParams {
        signers: vec![Address::new_id(1000).into(), Address::new_id(1001).into()],
        num_approvals_threshold: Default::default(),
        unlock_duration: Default::default(),
        start_epoch: Default::default(),
    };

    let multisig_propose_params = fil_actor_multisig_state::v16::ProposeParams {
        to: Address::new_id(1000).into(),
        value: Default::default(),
        method: 0,
        params: Default::default(),
    };

    let multisig_tx_id_params = fil_actor_multisig_state::v16::TxnIDParams {
        id: Default::default(),
        proposal_hash: vec![Default::default()],
    };

    let multisig_add_signer_params = fil_actor_multisig_state::v16::AddSignerParams {
        signer: Address::new_id(1012).into(),
        increase: false,
    };

    let multisig_remove_signer_params = fil_actor_multisig_state::v16::RemoveSignerParams {
        signer: Address::new_id(1012).into(),
        decrease: false,
    };

    let multisig_swap_signer_params = fil_actor_multisig_state::v16::SwapSignerParams {
        from: Address::new_id(122).into(),
        to: Address::new_id(1234).into(),
    };

    let multisig_change_num_app_params =
        fil_actor_multisig_state::v16::ChangeNumApprovalsThresholdParams { new_threshold: 2 };

    let multisig_lock_bal_params = fil_actor_multisig_state::v16::LockBalanceParams {
        start_epoch: 22,
        unlock_duration: 12,
        amount: Default::default(),
    };

    Ok(vec![
        RpcTest::identity(StateDecodeParams::request((
            Address::new_id(18101), // https://calibration.filscan.io/en/address/t018101/,
            fil_actor_multisig_state::v16::Method::Constructor as u64,
            to_vec(&multisig_constructor_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::new_id(18101), // https://calibration.filscan.io/en/address/t018101/,
            fil_actor_multisig_state::v16::Method::Propose as u64,
            to_vec(&multisig_propose_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::new_id(18101), // https://calibration.filscan.io/en/address/t018101/,
            fil_actor_multisig_state::v16::Method::Approve as u64,
            to_vec(&multisig_tx_id_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::new_id(18101), // https://calibration.filscan.io/en/address/t018101/,
            fil_actor_multisig_state::v16::Method::Cancel as u64,
            to_vec(&multisig_tx_id_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::new_id(18101), // https://calibration.filscan.io/en/address/t018101/,
            fil_actor_multisig_state::v16::Method::AddSigner as u64,
            to_vec(&multisig_add_signer_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::new_id(18101), // https://calibration.filscan.io/en/address/t018101/,
            fil_actor_multisig_state::v16::Method::RemoveSigner as u64,
            to_vec(&multisig_remove_signer_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::new_id(18101), // https://calibration.filscan.io/en/address/t018101/,
            fil_actor_multisig_state::v16::Method::SwapSigner as u64,
            to_vec(&multisig_swap_signer_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::new_id(18101), // https://calibration.filscan.io/en/address/t018101/,
            fil_actor_multisig_state::v16::Method::ChangeNumApprovalsThreshold as u64,
            to_vec(&multisig_change_num_app_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::new_id(18101), // https://calibration.filscan.io/en/address/t018101/,
            fil_actor_multisig_state::v16::Method::LockBalance as u64,
            to_vec(&multisig_lock_bal_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::new_id(18101), // https://calibration.filscan.io/en/address/t018101/,
            fil_actor_multisig_state::v16::Method::LockBalance as u64,
            to_vec(&multisig_lock_bal_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::new_id(18101), // https://calibration.filscan.io/en/address/t018101/,
            fil_actor_multisig_state::v16::Method::UniversalReceiverHook as u64,
            BASE64_STANDARD.decode("ghgqRBI0Vng=").unwrap(),
            tipset.key().into(),
        ))?),
    ])
}

// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

/// Creates state decode params tests for the Payment Channel actor.
pub fn create_tests(tipset: &Tipset) -> Result<Vec<RpcTest>> {
    // payment channel actor address `t066116`
    // https://calibration.filscan.io/en/address/t066116/
    let paych_address = Address::new_id(66116);

    let constructor_params = fil_actor_paych_state::v16::ConstructorParams {
        from: Address::new_id(1234).into(),
        to: Address::new_id(8457).into(),
    };

    let update_channel_state = fil_actor_paych_state::v16::UpdateChannelStateParams {
        sv: fil_actor_paych_state::v16::SignedVoucher {
            channel_addr: Address::new_id(1000).into(),
            time_lock_min: 21,
            time_lock_max: 234,
            secret_pre_image: vec![],
            extra: Some(fil_actor_paych_state::v16::ModVerifyParams {
                actor: Address::new_id(1234).into(),
                method: 223,
                data: Default::default(),
            }),
            lane: 234,
            nonce: 231,
            amount: Default::default(),
            min_settle_height: 0,
            merges: vec![],
            signature: None,
        },
        secret: vec![0x11, 0x22, 0x33, 0x44, 0x55], // dummy data
    };

    Ok(vec![
        RpcTest::identity(StateDecodeParams::request((
            paych_address,
            fil_actor_paych_state::v16::Method::Constructor as u64,
            to_vec(&constructor_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            paych_address,
            fil_actor_paych_state::v16::Method::UpdateChannelState as u64,
            to_vec(&update_channel_state)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            paych_address,
            fil_actor_paych_state::v16::Method::Settle as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            paych_address,
            fil_actor_paych_state::v16::Method::Collect as u64,
            vec![],
            tipset.key().into(),
        ))?),
    ])
}

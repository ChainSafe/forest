// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

/// Creates state decode params tests for the Datacap actor.
pub fn create_tests(tipset: &Tipset) -> Result<Vec<RpcTest>> {
    let datacap_constructor_params = fil_actor_datacap_state::v16::ConstructorParams {
        governor: Address::new_id(3000).into(),
    };

    let datacap_mint_params = fil_actor_datacap_state::v16::MintParams {
        to: Address::new_id(3001).into(),
        amount: TokenAmount::default().into(),
        operators: vec![Address::new_id(3002).into(), Address::new_id(3003).into()],
    };

    let datacap_destroy_params = fil_actor_datacap_state::v16::DestroyParams {
        owner: Address::new_id(3004).into(),
        amount: TokenAmount::default().into(),
    };

    let datacap_balance_params = fil_actor_datacap_state::v16::BalanceParams {
        address: Address::new_id(3005).into(),
    };

    let datacap_transfer_params = fil_actors_shared::frc46_token::token::types::TransferParams {
        to: Address::new_id(3006).into(),
        amount: TokenAmount::default().into(),
        operator_data: fvm_ipld_encoding::RawBytes::new(b"transfer test data".to_vec()),
    };

    let datacap_transfer_from_params =
        fil_actors_shared::frc46_token::token::types::TransferFromParams {
            from: Address::new_id(3007).into(),
            to: Address::new_id(3008).into(),
            amount: TokenAmount::default().into(),
            operator_data: fvm_ipld_encoding::RawBytes::new(b"transfer_from test data".to_vec()),
        };

    let datacap_increase_allowance_params =
        fil_actors_shared::frc46_token::token::types::IncreaseAllowanceParams {
            operator: Address::new_id(3009).into(),
            increase: TokenAmount::default().into(),
        };

    let datacap_decrease_allowance_params =
        fil_actors_shared::frc46_token::token::types::DecreaseAllowanceParams {
            operator: Address::new_id(3010).into(),
            decrease: TokenAmount::default().into(),
        };

    let datacap_revoke_allowance_params =
        fil_actors_shared::frc46_token::token::types::RevokeAllowanceParams {
            operator: Address::new_id(3011).into(),
        };

    let datacap_burn_params = fil_actors_shared::frc46_token::token::types::BurnParams {
        amount: TokenAmount::default().into(),
    };

    let datacap_burn_from_params = fil_actors_shared::frc46_token::token::types::BurnFromParams {
        owner: Address::new_id(3012).into(),
        amount: TokenAmount::default().into(),
    };

    let datacap_get_allowance_params =
        fil_actors_shared::frc46_token::token::types::GetAllowanceParams {
            owner: Address::new_id(3013).into(),
            operator: Address::new_id(3014).into(),
        };

    Ok(vec![
        RpcTest::identity(StateDecodeParams::request((
            Address::DATACAP_TOKEN_ACTOR,
            fil_actor_datacap_state::v16::Method::Constructor as u64,
            to_vec(&datacap_constructor_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::DATACAP_TOKEN_ACTOR,
            fil_actor_datacap_state::v16::Method::MintExported as u64,
            to_vec(&datacap_mint_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::DATACAP_TOKEN_ACTOR,
            fil_actor_datacap_state::v16::Method::DestroyExported as u64,
            to_vec(&datacap_destroy_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::DATACAP_TOKEN_ACTOR,
            fil_actor_datacap_state::v16::Method::NameExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::DATACAP_TOKEN_ACTOR,
            fil_actor_datacap_state::v16::Method::SymbolExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::DATACAP_TOKEN_ACTOR,
            fil_actor_datacap_state::v16::Method::TotalSupplyExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::DATACAP_TOKEN_ACTOR,
            fil_actor_datacap_state::v16::Method::BalanceExported as u64,
            to_vec(&datacap_balance_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::DATACAP_TOKEN_ACTOR,
            fil_actor_datacap_state::v16::Method::GranularityExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::DATACAP_TOKEN_ACTOR,
            fil_actor_datacap_state::v16::Method::TransferExported as u64,
            to_vec(&datacap_transfer_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::DATACAP_TOKEN_ACTOR,
            fil_actor_datacap_state::v16::Method::TransferFromExported as u64,
            to_vec(&datacap_transfer_from_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::DATACAP_TOKEN_ACTOR,
            fil_actor_datacap_state::v16::Method::IncreaseAllowanceExported as u64,
            to_vec(&datacap_increase_allowance_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::DATACAP_TOKEN_ACTOR,
            fil_actor_datacap_state::v16::Method::DecreaseAllowanceExported as u64,
            to_vec(&datacap_decrease_allowance_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::DATACAP_TOKEN_ACTOR,
            fil_actor_datacap_state::v16::Method::RevokeAllowanceExported as u64,
            to_vec(&datacap_revoke_allowance_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::DATACAP_TOKEN_ACTOR,
            fil_actor_datacap_state::v16::Method::BurnExported as u64,
            to_vec(&datacap_burn_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::DATACAP_TOKEN_ACTOR,
            fil_actor_datacap_state::v16::Method::BurnFromExported as u64,
            to_vec(&datacap_burn_from_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::DATACAP_TOKEN_ACTOR,
            fil_actor_datacap_state::v16::Method::AllowanceExported as u64,
            to_vec(&datacap_get_allowance_params)?,
            tipset.key().into(),
        ))?),
    ])
}

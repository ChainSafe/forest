// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use fil_actors_shared::fvm_ipld_bitfield::BitField;

/// Creates state decode params tests for the Market actor.
pub fn create_tests(tipset: &Tipset) -> Result<Vec<RpcTest>> {
    fn create_deal_proposal(
        client: fvm_shared4::address::Address,
        provider: fvm_shared4::address::Address,
        client_collateral: fvm_shared4::econ::TokenAmount,
        provider_collateral: fvm_shared4::econ::TokenAmount,
        start_epoch: fvm_shared4::clock::ChainEpoch,
        end_epoch: fvm_shared4::clock::ChainEpoch,
    ) -> fil_actor_market_state::v16::DealProposal {
        let piece_cid = Cid::default();
        let piece_size = fvm_shared4::piece::PaddedPieceSize(2048);
        let storage_price_per_epoch = fvm_shared4::econ::TokenAmount::from_atto(10u8);

        fil_actor_market_state::v16::DealProposal {
            piece_cid,
            piece_size,
            verified_deal: false,
            client,
            provider,
            label: fil_actor_market_state::v16::Label::String("label".to_string()),
            start_epoch,
            end_epoch,
            storage_price_per_epoch,
            provider_collateral,
            client_collateral,
        }
    }

    fn create_client_deal_proposal() -> fil_actor_market_state::v16::ClientDealProposal {
        let proposal = create_deal_proposal(
            fvm_shared4::address::Address::new_id(1000),
            fvm_shared4::address::Address::new_id(1000),
            fvm_shared4::econ::TokenAmount::from_atto(10u8),
            fvm_shared4::econ::TokenAmount::from_atto(10u8),
            0,
            10,
        );
        fil_actor_market_state::v16::ClientDealProposal {
            proposal,
            client_signature: fvm_shared4::crypto::signature::Signature::new_bls(
                b"test_signature".to_vec(),
            ),
        }
    }

    fn create_sector_deals() -> fil_actor_market_state::v16::SectorDeals {
        fil_actor_market_state::v16::SectorDeals {
            sector_number: 42,
            sector_type: fvm_shared4::sector::RegisteredSealProof::StackedDRG2KiBV1,
            sector_expiry: 100,
            deal_ids: vec![0, 1],
        }
    }

    fn create_sector_changes() -> fil_actor_miner_state::v16::SectorChanges {
        let piece_change = fil_actor_miner_state::v16::PieceChange {
            data: Cid::default(),
            size: fvm_shared4::piece::PaddedPieceSize(2048),
            payload: fvm_ipld_encoding::RawBytes::new(vec![0x12, 0x34, 0x56, 0x78]),
        };

        fil_actor_miner_state::v16::SectorChanges {
            sector: 2,
            minimum_commitment_epoch: 0,
            added: vec![piece_change],
        }
    }

    let market_actor_add_balance_params = fil_actor_market_state::v16::AddBalanceParams {
        provider_or_client: fvm_shared4::address::Address::new_id(1000),
    };
    let market_actor_withdraw_balance_params = fil_actor_market_state::v16::WithdrawBalanceParams {
        provider_or_client: Address::new_id(1000).into(),
        amount: TokenAmount::default().into(),
    };

    let market_actor_publish_storage_deals_params =
        fil_actor_market_state::v16::PublishStorageDealsParams {
            deals: vec![create_client_deal_proposal()],
        };

    let _market_actor_verify_deals_for_activation_params =
        fil_actor_market_state::v16::VerifyDealsForActivationParams {
            sectors: vec![create_sector_deals()],
        };

    let _market_actor_batch_activate_deals_params =
        fil_actor_market_state::v16::BatchActivateDealsParams {
            sectors: vec![create_sector_deals()],
            compute_cid: true,
        };

    let _market_actor_on_miner_sectors_terminate_params =
        fil_actor_market_state::v16::OnMinerSectorsTerminateParams {
            epoch: 123,
            sectors: {
                let mut bf = BitField::new();
                bf.set(3);
                bf
            },
        };

    let market_actor_get_balance_exported_params = Address::new_id(1000);

    let market_actor_settle_deal_payments_params =
        fil_actor_market_state::v16::SettleDealPaymentsParams {
            deal_ids: {
                let mut bf = BitField::new();
                bf.set(42);
                bf
            },
        };

    let market_actor_get_deal_data_commitment_params =
        fil_actor_market_state::v16::DealQueryParams { id: 0 };

    let _market_actor_sector_content_changed_params = {
        fil_actor_miner_state::v16::SectorContentChangedParams {
            sectors: vec![create_sector_changes()],
        }
    };

    Ok(vec![
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::AddBalance as u64,
            to_vec(&market_actor_add_balance_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::WithdrawBalance as u64,
            to_vec(&market_actor_withdraw_balance_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::PublishStorageDeals as u64,
            to_vec(&market_actor_publish_storage_deals_params)?,
            tipset.key().into(),
        ))?),
        // TODO(go-state-types): https://github.com/filecoin-project/go-state-types/issues/409
        // Enable this test when lotus supports this method
        // RpcTest::identity(StateDecodeParams::request((
        //     Address::MARKET_ACTOR,
        //     fil_actor_market_state::v16::Method::BatchActivateDeals as u64,
        //     to_vec(&market_actor_batch_activate_deals_params)?,
        //     tipset.key().into(),
        // ))?),
        // TODO(go-state-types): https://github.com/filecoin-project/go-state-types/issues/408
        // Enable this test once Lotus adds the `sector_number` field.
        // RpcTest::identity(StateDecodeParams::request((
        //     Address::MARKET_ACTOR,
        //     fil_actor_market_state::v16::Method::VerifyDealsForActivation as u64,
        //     to_vec(&market_actor_verify_deals_for_activation_params)?,
        //     tipset.key().into(),
        // ))?),
        // TODO(go-state-types): https://github.com/filecoin-project/go-state-types/issues/408
        // Enable this test when lotus supports correct types in go-state-types.
        // RpcTest::identity(StateDecodeParams::request((
        //     Address::MARKET_ACTOR,
        //     fil_actor_market_state::v16::Method::OnMinerSectorsTerminate as u64,
        //     to_vec(&market_actor_on_miner_sectors_terminate_params)?,
        //     tipset.key().into(),
        // ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::Constructor as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::CronTick as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::AddBalanceExported as u64,
            to_vec(&market_actor_get_balance_exported_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::WithdrawBalanceExported as u64,
            to_vec(&market_actor_withdraw_balance_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::PublishStorageDealsExported as u64,
            to_vec(&market_actor_publish_storage_deals_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::GetBalanceExported as u64,
            to_vec(&market_actor_get_balance_exported_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::GetDealDataCommitmentExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::GetDealClientExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::GetDealProviderExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::GetDealLabelExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::GetDealTermExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::GetDealTotalPriceExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::GetDealClientCollateralExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::GetDealProviderCollateralExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::GetDealVerifiedExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::GetDealActivationExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::GetDealSectorExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            fil_actor_market_state::v16::Method::SettleDealPaymentsExported as u64,
            to_vec(&market_actor_settle_deal_payments_params)?,
            tipset.key().into(),
        ))?),
        // TODO(lotus): https://github.com/filecoin-project/lotus/issues/13329
        // Lotus panics while decoding this method.
        // RpcTest::identity(StateDecodeParams::request((
        //     Address::MARKET_ACTOR,
        //     fil_actor_market_state::v16::Method::SectorContentChangedExported as u64,
        //     to_vec(&market_actor_sector_content_changed_params)?,
        //     tipset.key().into(),
        // ))?),
    ])
}

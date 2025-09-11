// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use fil_actor_market_state::v17::*;
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
    ) -> DealProposal {
        let piece_cid = Cid::default();
        let piece_size = fvm_shared4::piece::PaddedPieceSize(2048);
        let storage_price_per_epoch = fvm_shared4::econ::TokenAmount::from_atto(10u8);

        DealProposal {
            piece_cid,
            piece_size,
            verified_deal: false,
            client,
            provider,
            label: Label::String("label".to_string()),
            start_epoch,
            end_epoch,
            storage_price_per_epoch,
            provider_collateral,
            client_collateral,
        }
    }

    fn create_client_deal_proposal() -> ClientDealProposal {
        let proposal = create_deal_proposal(
            fvm_shared4::address::Address::new_id(1000),
            fvm_shared4::address::Address::new_id(1000),
            fvm_shared4::econ::TokenAmount::from_atto(10u8),
            fvm_shared4::econ::TokenAmount::from_atto(10u8),
            0,
            10,
        );
        ClientDealProposal {
            proposal,
            client_signature: fvm_shared4::crypto::signature::Signature::new_bls(
                b"test_signature".to_vec(),
            ),
        }
    }

    fn create_sector_deals() -> SectorDeals {
        SectorDeals {
            sector_number: 42,
            sector_type: fvm_shared4::sector::RegisteredSealProof::StackedDRG2KiBV1,
            sector_expiry: 100,
            deal_ids: vec![0, 1],
        }
    }

    fn create_sector_changes() -> ext::miner::SectorChanges {
        let piece_change = ext::miner::PieceChange {
            data: Cid::default(),
            size: fvm_shared4::piece::PaddedPieceSize(2048),
            payload: fvm_ipld_encoding::RawBytes::new(vec![0x12, 0x34, 0x56, 0x78]),
        };

        ext::miner::SectorChanges {
            sector: 2,
            minimum_commitment_epoch: 0,
            added: vec![piece_change],
        }
    }

    let market_actor_add_balance_params = AddBalanceParams {
        provider_or_client: fvm_shared4::address::Address::new_id(1000),
    };
    let market_actor_withdraw_balance_params = WithdrawBalanceParams {
        provider_or_client: Address::new_id(1000).into(),
        amount: TokenAmount::default().into(),
    };

    let market_actor_publish_storage_deals_params = PublishStorageDealsParams {
        deals: vec![create_client_deal_proposal()],
    };

    let _market_actor_verify_deals_for_activation_params = VerifyDealsForActivationParams {
        sectors: vec![create_sector_deals()],
    };

    let _market_actor_batch_activate_deals_params = BatchActivateDealsParams {
        sectors: vec![create_sector_deals()],
        compute_cid: true,
    };

    let _market_actor_on_miner_sectors_terminate_params = OnMinerSectorsTerminateParams {
        epoch: 123,
        sectors: {
            let mut bf = BitField::new();
            bf.set(3);
            bf
        },
    };

    let market_actor_get_balance_exported_params = Address::new_id(1000);

    let market_actor_settle_deal_payments_params = SettleDealPaymentsParams {
        deal_ids: {
            let mut bf = BitField::new();
            bf.set(42);
            bf
        },
    };

    let market_actor_get_deal_data_commitment_params = DealQueryParams { id: 0 };

    let _market_actor_sector_content_changed_params = {
        ext::miner::SectorContentChangedParams {
            sectors: vec![create_sector_changes()],
        }
    };

    Ok(vec![
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::AddBalance as u64,
            to_vec(&market_actor_add_balance_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::WithdrawBalance as u64,
            to_vec(&market_actor_withdraw_balance_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::PublishStorageDeals as u64,
            to_vec(&market_actor_publish_storage_deals_params)?,
            tipset.key().into(),
        ))?),
        // TODO(go-state-types): https://github.com/filecoin-project/go-state-types/issues/409
        // Enable this test when lotus supports this method
        // RpcTest::identity(StateDecodeParams::request((
        //     Address::MARKET_ACTOR,
        //     Method::BatchActivateDeals as u64,
        //     to_vec(&market_actor_batch_activate_deals_params)?,
        //     tipset.key().into(),
        // ))?),
        // TODO(go-state-types): https://github.com/filecoin-project/go-state-types/issues/408
        // Enable this test once Lotus adds the `sector_number` field.
        // RpcTest::identity(StateDecodeParams::request((
        //     Address::MARKET_ACTOR,
        //     Method::VerifyDealsForActivation as u64,
        //     to_vec(&market_actor_verify_deals_for_activation_params)?,
        //     tipset.key().into(),
        // ))?),
        // TODO(go-state-types): https://github.com/filecoin-project/go-state-types/issues/408
        // Enable this test when lotus supports correct types in go-state-types.
        // RpcTest::identity(StateDecodeParams::request((
        //     Address::MARKET_ACTOR,
        //     Method::OnMinerSectorsTerminate as u64,
        //     to_vec(&market_actor_on_miner_sectors_terminate_params)?,
        //     tipset.key().into(),
        // ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::Constructor as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::CronTick as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::AddBalanceExported as u64,
            to_vec(&market_actor_get_balance_exported_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::WithdrawBalanceExported as u64,
            to_vec(&market_actor_withdraw_balance_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::PublishStorageDealsExported as u64,
            to_vec(&market_actor_publish_storage_deals_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::GetBalanceExported as u64,
            to_vec(&market_actor_get_balance_exported_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::GetDealDataCommitmentExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::GetDealClientExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::GetDealProviderExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::GetDealLabelExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::GetDealTermExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::GetDealTotalPriceExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::GetDealClientCollateralExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::GetDealProviderCollateralExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::GetDealVerifiedExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::GetDealActivationExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::GetDealSectorExported as u64,
            to_vec(&market_actor_get_deal_data_commitment_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::MARKET_ACTOR,
            Method::SettleDealPaymentsExported as u64,
            to_vec(&market_actor_settle_deal_payments_params)?,
            tipset.key().into(),
        ))?),
        // TODO(lotus): https://github.com/filecoin-project/lotus/issues/13329
        // Lotus panics while decoding this method.
        // RpcTest::identity(StateDecodeParams::request((
        //     Address::MARKET_ACTOR,
        //     Method::SectorContentChangedExported as u64,
        //     to_vec(&market_actor_sector_content_changed_params)?,
        //     tipset.key().into(),
        // ))?),
    ])
}

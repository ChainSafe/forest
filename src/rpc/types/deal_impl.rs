// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

impl From<DealState> for ApiDealState {
    fn from(s: DealState) -> Self {
        let DealState {
            sector_start_epoch,
            last_updated_epoch,
            slash_epoch,
            verified_claim,
            sector_number,
        } = s;
        Self {
            sector_start_epoch,
            last_updated_epoch,
            slash_epoch,
            verified_claim,
            sector_number,
        }
    }
}

impl From<DealProposal> for ApiDealProposal {
    fn from(p: DealProposal) -> Self {
        let DealProposal {
            piece_cid,
            piece_size,
            verified_deal,
            client,
            provider,
            label,
            start_epoch,
            end_epoch,
            storage_price_per_epoch,
            provider_collateral,
            client_collateral,
        } = p;
        Self {
            piece_cid,
            piece_size: piece_size.0,
            verified_deal,
            client: client.into(),
            provider: provider.into(),
            label,
            start_epoch,
            end_epoch,
            storage_price_per_epoch: storage_price_per_epoch.into(),
            provider_collateral: provider_collateral.into(),
            client_collateral: client_collateral.into(),
        }
    }
}

impl From<MarketDeal> for ApiMarketDeal {
    fn from(d: MarketDeal) -> Self {
        Self {
            proposal: d.proposal.into(),
            state: d.state.into(),
        }
    }
}

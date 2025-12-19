// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use crate::shim::{address::Address, clock::ChainEpoch, econ::TokenAmount};
use fil_actor_miner_state::v12::PendingBeneficiaryChange;

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "PendingBeneficiaryChange")]
pub struct PendingBeneficiaryChangeLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub new_beneficiary: Address,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub new_quota: TokenAmount,
    pub new_expiration: ChainEpoch,
    pub approved_by_beneficiary: bool,
    pub approved_by_nominee: bool,
}

impl HasLotusJson for PendingBeneficiaryChange {
    type LotusJson = PendingBeneficiaryChangeLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "NewBeneficiary": "f00",
                "NewQuota": "0",
                "NewExpiration": 0,
                "ApprovedByBeneficiary": false,
                "ApprovedByNominee": false,
            }),
            Self {
                new_beneficiary: Default::default(),
                new_quota: Default::default(),
                new_expiration: Default::default(),
                approved_by_beneficiary: Default::default(),
                approved_by_nominee: Default::default(),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        PendingBeneficiaryChangeLotusJson {
            new_beneficiary: self.new_beneficiary.into(),
            new_quota: self.new_quota.into(),
            new_expiration: self.new_expiration,
            approved_by_beneficiary: self.approved_by_beneficiary,
            approved_by_nominee: self.approved_by_nominee,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            new_beneficiary: lotus_json.new_beneficiary.into(),
            new_quota: lotus_json.new_quota.into(),
            new_expiration: lotus_json.new_expiration,
            approved_by_beneficiary: lotus_json.approved_by_beneficiary,
            approved_by_nominee: lotus_json.approved_by_nominee,
        }
    }
}

#[test]
fn snapshots() {
    assert_all_snapshots::<PendingBeneficiaryChange>();
}

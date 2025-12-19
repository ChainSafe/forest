// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::{clock::ChainEpoch, econ::TokenAmount};
use fil_actor_miner_state::v12::BeneficiaryTerm;

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "BeneficiaryTerm")]
pub struct BeneficiaryTermLotusJson {
    /// The total amount the current beneficiary can withdraw. Monotonic, but reset when beneficiary changes.
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub quota: TokenAmount,
    /// The amount of quota the current beneficiary has already withdrawn
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub used_quota: TokenAmount,
    /// The epoch at which the beneficiary's rights expire and revert to the owner
    pub expiration: ChainEpoch,
}

impl HasLotusJson for BeneficiaryTerm {
    type LotusJson = BeneficiaryTermLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Quota": "0",
                "UsedQuota": "0",
                "Expiration": 0,
            }),
            Default::default(),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        BeneficiaryTermLotusJson {
            used_quota: self.used_quota.into(),
            quota: self.quota.into(),
            expiration: self.expiration,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            used_quota: lotus_json.used_quota.into(),
            quota: lotus_json.quota.into(),
            expiration: lotus_json.expiration,
        }
    }
}

#[test]
fn snapshots() {
    assert_all_snapshots::<BeneficiaryTerm>();
}

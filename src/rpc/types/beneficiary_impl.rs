// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

// TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/4032
//                  this should move to src/lotus_json
impl HasLotusJson for BeneficiaryTerm {
    type LotusJson = BeneficiaryTermLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(Value, Self)> {
        vec![]
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

// TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/4032
//                  this should move to src/lotus_json
impl HasLotusJson for PendingBeneficiaryChange {
    type LotusJson = PendingBeneficiaryChangeLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(Value, Self)> {
        vec![]
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

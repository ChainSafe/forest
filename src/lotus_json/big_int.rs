// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use num::BigInt;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "BigInt")]
pub struct BigIntLotusJson(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::stringify")]
    BigInt,
);

impl HasLotusJson for BigInt {
    type LotusJson = BigIntLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("1"), BigInt::from(1))]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        BigIntLotusJson(self)
    }

    fn from_lotus_json(BigIntLotusJson(big_int): Self::LotusJson) -> Self {
        big_int
    }
}

macro_rules! impl_bigint_de {
    ($($bigint_de_type:ty),+) => {
        $(
            impl HasLotusJson for $bigint_de_type {
                type LotusJson = BigIntLotusJson;

                #[cfg(test)]
                fn snapshots() -> Vec<(serde_json::Value, Self)> {
                    vec![(json!("1000"), Self(BigInt::from(1000)))]
                }

                fn into_lotus_json(self) -> Self::LotusJson {
                    BigIntLotusJson(self.0)
                }

                fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                    let BigIntLotusJson(big_int) = lotus_json;
                    Self(big_int)
                }
            }
        )+
    };
}

impl_bigint_de!(
    fvm_shared2::bigint::bigint_ser::BigIntDe,
    fvm_shared3::bigint::bigint_ser::BigIntDe,
    fvm_shared4::bigint::bigint_ser::BigIntDe
);

use super::*;

use crate::shim::econ::TokenAmount;

#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct TokenAmountLotusJson {
    #[serde(with = "stringify")]
    attos: num::BigInt,
}

impl HasLotusJson for TokenAmount {
    type LotusJson = TokenAmountLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("1"), TokenAmount::from_atto(1))]
    }
}

impl From<TokenAmount> for TokenAmountLotusJson {
    fn from(value: TokenAmount) -> Self {
        Self {
            attos: value.atto().clone(),
        }
    }
}

impl From<TokenAmountLotusJson> for TokenAmount {
    fn from(value: TokenAmountLotusJson) -> Self {
        Self::from_atto(value.attos)
    }
}

#[cfg(test)]
quickcheck! {
    fn round_trip(val: TokenAmount) -> bool {
        assert_via_json(val);
        true
    }
}

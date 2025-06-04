use super::*;
use ::cid::Cid;
use fil_actors_shared::frc46_token::token;
use serde_json::Value;
use crate::shim::econ::TokenAmount;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "TokenState")]
pub struct TokenStateLotusJson {
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub supply: TokenAmount,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub balances: Cid,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub allowances: Cid,

    pub hamt_bit_width: u32,
}

impl HasLotusJson for token::state::TokenState {
    type LotusJson = TokenStateLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(Value, Self)> {
        todo!()
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        TokenStateLotusJson {
            supply: self.supply.into(),
            balances: self.balances,
            allowances: self.allowances,
            hamt_bit_width: self.hamt_bit_width,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        token::state::TokenState {
            supply: lotus_json.supply.into(),
            balances: lotus_json.balances,
            allowances: lotus_json.allowances,
            hamt_bit_width: lotus_json.hamt_bit_width,
        }
    }
}
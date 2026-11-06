// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use crate::shim::bigint::MAX_BIGINT_BITS;
use num::BigInt;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "BigInt")]
pub struct BigIntLotusJson(
    #[schemars(with = "String")]
    #[serde(
        serialize_with = "crate::lotus_json::stringify::serialize",
        deserialize_with = "deserialize_decimal"
    )]
    BigInt,
);

/// Well above the 307 characters of a signed [`MAX_BIGINT_BITS`] value; bounds the quadratic decimal conversion.
const MAX_DECIMAL_LEN: usize = 512;

#[derive(Debug, PartialEq, thiserror::Error)]
enum ParseDecimalError {
    #[error("big integer string exceeds {MAX_DECIMAL_LEN} characters")]
    TooLong,
    #[error(transparent)]
    Invalid(#[from] num::bigint::ParseBigIntError),
    #[error("big integer exceeds {MAX_BIGINT_BITS} bits")]
    TooManyBits,
}

fn parse_decimal(s: &str) -> Result<BigInt, ParseDecimalError> {
    if s.len() > MAX_DECIMAL_LEN {
        return Err(ParseDecimalError::TooLong);
    }
    // Every realistic on-chain amount fits `i128`, which parses about twice as fast as num-bigint.
    if let Ok(small) = s.parse::<i128>() {
        return Ok(small.into());
    }
    let value: BigInt = s.parse()?;
    if value.bits() > MAX_BIGINT_BITS {
        return Err(ParseDecimalError::TooManyBits);
    }
    Ok(value)
}

fn deserialize_decimal<'de, D: Deserializer<'de>>(deserializer: D) -> Result<BigInt, D::Error> {
    parse_decimal(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lotus_json::assert_unchanged_via_json;
    use pretty_assertions::assert_eq;
    use quickcheck_macros::quickcheck;
    use rstest::rstest;

    crate::test_snapshots!(BigInt);

    #[rstest]
    fn accepts_exactly_what_cbor_can_encode(
        #[values(MAX_BIGINT_BITS, MAX_BIGINT_BITS + 1)] bits: u64,
        #[values(false, true)] negative: bool,
    ) {
        let magnitude: BigInt = (BigInt::from(1) << bits) - 1;
        let value = if negative { -magnitude } else { magnitude };
        let encodable =
            fvm_ipld_encoding::to_vec(&crate::shim::econ::TokenAmount::from_atto(value.clone()))
                .is_ok();
        let parsed = serde_json::from_value::<BigIntLotusJson>(json!(value.to_string())).ok();
        assert_eq!(parsed.map(|p| p.0), encodable.then_some(value));
    }

    #[quickcheck]
    fn round_trips(amount: crate::shim::econ::TokenAmount, negative: bool) {
        let value = amount.atto().clone();
        assert_unchanged_via_json(if negative { -value } else { value });
    }

    #[test]
    fn caps_length_before_converting() {
        assert_eq!(
            parse_decimal(&"9".repeat(MAX_DECIMAL_LEN + 1)),
            Err(ParseDecimalError::TooLong)
        );
    }
}

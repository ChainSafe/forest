// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::percent::Percent;

impl HasLotusJson for Percent {
    type LotusJson = f64;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!(1.25), Percent(125)), (json!(1.10), Percent(110))]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.0 as f64 / 100.0
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        // Scaling via the decimal representation avoids `1.10 * 100.0 == 110.00000000000001`.
        let scaled = format!("{lotus_json}e2").parse::<f64>().unwrap_or(f64::NAN);
        // Lotus rejects these outright, but this conversion is infallible by trait contract.
        if !scaled.is_finite() || scaled < 0.0 || scaled.trunc() != scaled {
            tracing::warn!("ratio must be a non-negative multiple of 0.01, coercing: {lotus_json}");
        }
        Percent(scaled as u64) // saturates at both ends, NaN maps to 0
    }
}

#[cfg(test)]
impl quickcheck::Arbitrary for Percent {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let whole = u32::arbitrary(g) % 10_000;
        let frac = u32::arbitrary(g) % 100;
        Percent(u64::from(whole) * 100 + u64::from(frac))
    }
}

#[cfg(test)]
mod tests {
    use super::{HasLotusJson, Percent};
    use rstest::rstest;

    #[rstest]
    #[case::typical(1.25, 125)]
    #[case::trailing_zero(1.10, 110)]
    #[case::one(1.0, 100)]
    #[case::zero(0.0, 0)]
    #[case::negative_zero(-0.0, 0)]
    #[case::negative(-1.5, 0)]
    #[case::excess_precision_truncates(1.255, 125)]
    #[case::below_precision(0.009, 0)]
    #[case::nan(f64::NAN, 0)]
    #[case::infinity(f64::INFINITY, 0)]
    #[case::negative_infinity(f64::NEG_INFINITY, 0)]
    #[case::overflow_saturates(1e300, u64::MAX)]
    fn from_lotus_json_is_total(#[case] ratio: f64, #[case] expected: u64) {
        assert_eq!(Percent::from_lotus_json(ratio), Percent(expected));
    }

    #[rstest]
    #[case(0)]
    #[case(1)]
    #[case(125)]
    #[case(u64::MAX)]
    fn round_trips(#[case] percent: u64) {
        let percent = Percent(percent);
        assert_eq!(Percent::from_lotus_json(percent.into_lotus_json()), percent);
    }
}

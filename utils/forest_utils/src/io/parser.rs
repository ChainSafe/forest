// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use forest_shim::econ::TokenAmount;
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;

const NUM_SIGNIFICANT_DIGITS: u32 = 4;

pub fn parse_duration(arg: &str) -> anyhow::Result<Duration> {
    let seconds = arg.parse()?;
    Ok(Duration::from_secs(seconds))
}

#[allow(clippy::enum_variant_names)]
pub enum FormattingMode {
    /// mode to show data in `FIL` units
    /// in full accuracy
    /// E.g. 0.50023677980 `FIL`
    ExactFixed,
    /// mode to show data in `FIL` units
    /// with 4 significant digits
    /// E.g. 0.5002 `FIL`
    NotExactFixed,
    /// mode to show data in SI units
    /// in full accuracy
    /// E.g. 500.2367798 `milli FIL`
    ExactNotFixed,
    /// mode to show data in SI units
    /// with 4 significant digits
    /// E.g. ~500.2 milli `FIL`
    NotExactNotFixed,
}

pub fn bool_pair_to_mode(exact: bool, fixed: bool) -> FormattingMode {
    if exact && fixed {
        FormattingMode::ExactFixed
    } else if !exact && fixed {
        FormattingMode::NotExactFixed
    } else if exact && !fixed {
        FormattingMode::ExactNotFixed
    } else {
        FormattingMode::NotExactNotFixed
    }
}

/// Function to format `TokenAmount` accoding to `FormattingMode`:
/// mode to show data in `FIL` units
/// in full accuracy for `ExactFixed` mode,
/// mode to show data in `FIL` units
/// with 4 significant digits for `NotExactFixed` mode,
/// mode to show data in SI units
/// in full accuracy for `ExactNotFixed` mode,
/// mode to show data in SI units
/// with 4 significant digits for `NotExactNotFixed` mode
pub fn format_balance_string(
    token_amount: TokenAmount,
    mode: FormattingMode,
) -> anyhow::Result<String> {
    // all SI prefixes we support currently
    let units = ["atto ", "femto ", "pico ", "nano ", "micro ", "milli ", ""];
    // get `TokenAmount`.atto() as a `Decimal` for further formatting
    let num: Decimal = Decimal::try_from_i128_with_scale(
        token_amount
            .atto()
            .to_i128()
            // currently the amount cannot be more than 2B x 10^18 atto FIL
            // the limit here is 2^96 atto FIL
            .ok_or(anyhow::Error::msg(
                "Number exceeds maximum value that can be represented.",
            ))?,
        0,
    )?;

    let orig = num;

    let mut num = num;
    let mut unit_index = 0;
    // find the right SI prefix and divide the amount of tokens accordingly
    while num >= dec!(1000.0) && unit_index < units.len() - 1 {
        num /= dec!(1000.0);
        unit_index += 1;
    }

    let res = match mode {
        FormattingMode::ExactFixed => {
            let fil = orig / dec!(1e18);
            // format the data in full accuracy in `FIL`
            format!("{fil} FIL")
        }
        FormattingMode::NotExactFixed => {
            let fil_orig = orig / dec!(1e18);
            let fil = fil_orig
                .round_sf_with_strategy(
                    NUM_SIGNIFICANT_DIGITS,
                    RoundingStrategy::MidpointAwayFromZero,
                )
                .ok_or(anyhow::Error::msg("cannot represent"))?;
            // format the data with 4 significant digits in `FIL``
            let mut res = format!("{fil} FIL");
            // if the rounding actually loses any information we need to indicate it
            if fil != fil_orig {
                res.insert(0, '~');
            }
            res
        }
        FormattingMode::ExactNotFixed => format!("{num:0} {}FIL", units[unit_index]),
        FormattingMode::NotExactNotFixed => {
            let mut fil = num
                .round_sf_with_strategy(
                    NUM_SIGNIFICANT_DIGITS,
                    RoundingStrategy::MidpointAwayFromZero,
                )
                .ok_or(anyhow::Error::msg("cannot represent"))?;
            if fil == fil.trunc() {
                fil = fil.trunc();
            }
            // format the data with 4 significant digits in SI units
            let mut res = format!("{} {}FIL", fil, units[unit_index]);

            // if the rounding actually loses any information we need to indicate it
            if fil != num {
                res.insert(0, '~');
            }

            res
        }
    };
    Ok(res)
}

#[cfg(test)]
mod test {
    use forest_shim::econ::TokenAmount;

    use super::*;

    #[test]
    fn exact_balance_fixed_unit() {
        let cases_vec = vec![
            (100, "0.0000000000000001 FIL"),
            (12465, "0.000000000000012465 FIL"),
            (500236779800000000, "0.50023677980 FIL"),
            (1508900000000005000, "1.508900000000005 FIL"),
        ];

        for (atto, result) in cases_vec {
            test_call(atto, result, true, true);
        }
    }

    #[test]
    fn not_exact_balance_fixed_unit() {
        let cases_vec = vec![
            (100, "0.0000000000000001000 FIL"),
            (999999999999999999, "~1.0000 FIL"),
            (1000005000, "~0.000000001000 FIL"),
            (508900000000005000, "~0.5089 FIL"),
            (1508900000000005000, "~1.509 FIL"),
            (2508900009000005000, "~2.509 FIL"),
        ];

        for (atto, result) in cases_vec {
            test_call(atto, result, false, true);
        }
    }

    #[test]
    fn exact_balance_not_fixed_unit() {
        let cases_vec = vec![
            (100, "100 atto FIL"),
            (120005, "120.005 femto FIL"),
            (200000045, "200.000045 pico FIL"),
            (1000000123, "1.000000123 nano FIL"),
            (450000008000000, "450.000008 micro FIL"),
            (90000002750000000, "90.00000275 milli FIL"),
            (1508900000000005000, "1.508900000000005 FIL"),
            (2508900009000005000, "2.508900009000005 FIL"),
        ];

        for (atto, result) in cases_vec {
            test_call(atto, result, true, false);
        }
    }

    #[test]
    fn not_exact_balance_not_fixed_unit() {
        let cases_vec = vec![
            (100, "100 atto FIL"),
            (120005, "~120 femto FIL"),
            (200000045, "~200 pico FIL"),
            (1000000123, "~1 nano FIL"),
            (450000008000000, "~450 micro FIL"),
            (90000002750000000, "~90 milli FIL"),
            (500236779800000000, "~500.2 milli FIL"),
            (1508900000000005000, "~1.509 FIL"),
            (2508900009000005000, "~2.509 FIL"),
        ];

        for (atto, result) in cases_vec {
            test_call(atto, result, false, false);
        }
    }

    fn test_call(atto: i64, result: &str, exact: bool, fixed: bool) {
        assert_eq!(
            format_balance_string(
                TokenAmount::from_atto(atto),
                bool_pair_to_mode(exact, fixed)
            )
            .unwrap(),
            result
        );
    }

    #[test]
    fn test_too_big_value() {
        assert_eq!(
            format_balance_string(
                TokenAmount::from_whole(2508900009000005000000000000i128),
                bool_pair_to_mode(true, true)
            )
            .unwrap_err()
            .to_string(),
            "Number exceeds maximum value that can be represented."
        );
    }

    #[test]
    fn test_2_96_value() {
        assert_eq!(
            format_balance_string(
                TokenAmount::from_atto(79228162514264337593543950336i128),
                bool_pair_to_mode(true, true)
            )
            .unwrap_err()
            .to_string(),
            "Number exceeds maximum value that can be represented."
        );
    }
}

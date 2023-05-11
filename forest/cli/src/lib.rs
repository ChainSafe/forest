// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
pub mod cli;

pub mod humantoken {
    // ENHANCE(aatifsyed): could accept pairs like "1 nano 1 atto"

    use anyhow::{anyhow, bail};
    use bigdecimal::{BigDecimal, ParseBigDecimalError};
    use fvm_shared::econ::TokenAmount;
    use nom::{
        bytes::complete::tag,
        character::complete::multispace0,
        combinator::{map_res, opt},
        error::{FromExternalError, ParseError},
        number::complete::recognize_float,
        sequence::terminated,
        IResult,
    };

    // Use a struct as a table row instead of an enum
    // to make our code less macro-heavy
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct SIScale {
        /// "micro"
        name: &'static str,
        /// [ "μ", "u" ]
        units: &'static [&'static str],
        /// -6
        exponent: i8,
        /// "0.000001"
        multiplier: &'static str,
    }

    impl SIScale {
        // ENHANCE(aatifsyed): could memoize this if it's called in a hot loop
        fn multiplier(&self) -> BigDecimal {
            self.multiplier.parse().unwrap()
        }
    }

    macro_rules! define_scales {
        ($($name:ident $symbol:ident$(or $alt_symbol:ident)* $base_10:literal $decimal:literal),* $(,)?) => {

            // Define constants
            $(
                #[allow(non_upper_case_globals)]
                const $name: SIScale = SIScale {
                    name: stringify!($name),
                    units: &[stringify!($symbol) $(, stringify!($alt_symbol))* ],
                    exponent: $base_10,
                    multiplier: stringify!($decimal),
                };
            )*

            // Define top level array
            const ALL_SCALES: &[SIScale] =
                &[
                    $(
                        $name
                    ,)*
                ];

        };
    }

    define_scales! {
        quetta	Q	30	1000000000000000000000000000000,
        ronna	R	27	1000000000000000000000000000,
        yotta	Y	24	1000000000000000000000000,
        zetta	Z	21	1000000000000000000000,
        exa	    E	18	1000000000000000000,
        peta	P	15	1000000000000000,
        tera	T	12	1000000000000,
        giga	G	9	1000000000,
        mega	M	6	1000000,
        kilo	k	3	1000,
        hecto	h	2	100,
        deca	da	1	10,
        deci	d	-1	0.1,
        centi	c	-2	0.01,
        milli	m	-3	0.001,
        micro	μ or u	-6	0.000001,
        nano	n	-9	0.000000001,
        pico	p	-12	0.000000000001,
        femto	f	-15	0.000000000000001,
        atto	a	-18	0.000000000000000001,
        zepto	z	-21	0.000000000000000000001,
        yocto	y	-24	0.000000000000000000000001,
        ronto	r	-27	0.000000000000000000000000001,
        quecto	q	-30	0.000000000000000000000000000001,
    }

    fn nom2anyhow(e: nom::Err<nom::error::VerboseError<&str>>) -> anyhow::Error {
        anyhow!("parse error: {e}")
    }

    pub fn parse(input: &str) -> anyhow::Result<TokenAmount> {
        let (mut big_decimal, scale) = parse_big_decimal_and_scale(input)?;

        if let Some(scale) = scale {
            big_decimal *= scale.multiplier();
        }

        let fil = big_decimal;
        let attos = fil * atto.multiplier().inverse();

        if !attos.is_integer() {
            bail!("sub-atto amounts are not allowed");
        }

        let (attos, scale) = attos.with_scale(0).into_bigint_and_exponent();
        assert_eq!(scale, 0, "we've just set the scale!");

        Ok(TokenAmount::from_atto(attos))
    }

    fn parse_big_decimal_and_scale(input: &str) -> anyhow::Result<(BigDecimal, Option<SIScale>)> {
        // Strip `fil` or `FIL` at most once from the end
        let input = match (input.strip_suffix("FIL"), input.strip_suffix("fil")) {
            // remove whitespace before the units if there was any
            (Some(stripped), _) => stripped.trim_end(),
            (_, Some(stripped)) => stripped.trim_end(),
            _ => input,
        };

        let (input, big_decimal) = permit_trailing_ws(bigdecimal)(input).map_err(nom2anyhow)?;
        let (input, scale) = opt(permit_trailing_ws(si_scale))(input).map_err(nom2anyhow)?;

        if !input.is_empty() {
            bail!("Unexpected trailing input: {input}")
        }

        Ok((big_decimal, scale))
    }

    fn permit_trailing_ws<'a, F, O, E: ParseError<&'a str>>(
        inner: F,
    ) -> impl FnMut(&'a str) -> IResult<&'a str, O, E>
    where
        F: FnMut(&'a str) -> IResult<&'a str, O, E>,
    {
        terminated(inner, multispace0)
    }

    /// Take an SIScale from the front of `input`
    fn si_scale<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&str, SIScale, E> {
        // Try the longest matches first, so we don't e.g match `a` instead of `atto`,
        // leaving `tto`.

        let mut scales = ALL_SCALES
            .iter()
            .flat_map(|scale| {
                std::iter::once(&scale.name)
                    .chain(scale.units)
                    .map(move |prefix| (*prefix, scale))
            })
            .collect::<Vec<_>>();
        scales.sort_by_key(|(prefix, _)| std::cmp::Reverse(*prefix));

        for (prefix, scale) in scales {
            if let Ok((rem, _prefix)) = tag::<_, _, E>(prefix)(input) {
                return Ok((rem, *scale));
            }
        }

        Err(nom::Err::Error(E::from_error_kind(
            input,
            nom::error::ErrorKind::Alt,
        )))
    }

    /// Take a float from the front of `input`
    fn bigdecimal<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&str, BigDecimal, E>
    where
        E: FromExternalError<&'a str, ParseBigDecimalError>,
    {
        map_res(recognize_float, str::parse)(input)
    }

    #[cfg(test)]
    mod tests {
        use std::str::FromStr;

        use super::*;

        #[test]
        fn cover_scales() {
            for scale in ALL_SCALES {
                let _did_not_panic = scale.multiplier();
            }
        }

        #[test]
        fn parse_bigdecimal() {
            fn do_test(input: &str, expected: &str) {
                let expected = BigDecimal::from_str(expected).unwrap();
                let (rem, actual) = bigdecimal::<nom::error::VerboseError<_>>(input).unwrap();
                assert_eq!(expected, actual);
                assert!(rem.is_empty());
            }
            do_test("1", "1");
            do_test("0.1", "0.1");
            do_test(".1", ".1");
            do_test("1e1", "10");
            do_test("1.", "1");
        }

        fn do_test(input: &str, expected_amount: &str, expected_scale: impl Into<Option<SIScale>>) {
            let expected_amount = BigDecimal::from_str(expected_amount).unwrap();
            let expected_scale = expected_scale.into();
            let (actual_amount, actual_scale) = parse_big_decimal_and_scale(input).unwrap();
            assert_eq!(expected_amount, actual_amount, "{input}");
            assert_eq!(expected_scale, actual_scale, "{input}");
        }

        #[test]
        fn basic_bigdecimal_and_scale() {
            // plain
            do_test("1", "1", None);

            // include unit
            do_test("1 FIL", "1", None);
            do_test("1FIL", "1", None);
            do_test("1 fil", "1", None);
            do_test("1fil", "1", None);

            let possible_units = ["", "fil", "FIL", " fil", " FIL"];
            let possible_prefixes = ["atto", "a", " atto", " a"];

            for unit in possible_units {
                for prefix in possible_prefixes {
                    let input = format!("1{prefix}{unit}");
                    do_test(&input, "1", atto)
                }
            }
        }

        #[test]
        fn parse_exa_and_exponent() {
            do_test("1 E", "1", exa);
            do_test("1e0E", "1", exa);

            // ENHANCE(aatifsyed): this should be parsed as 1 exa, but that
            // would probably require an entirely custom float parser with
            // lookahead - users will have to include a space for now

            // do_test("1E", "1", exa);
        }

        #[test]
        fn more_than_69_bits() {
            // (nice)
            // The previous rust_decimal implementation had at most 69 bits of precision
            // we should be able to exceed that
            let mut test_str = String::with_capacity(100);
            test_str.push('1');
            for _ in 0..98 {
                test_str.push('0')
            }
            test_str.push('1');
            do_test(&test_str, &test_str, None);
        }

        #[test]
        fn disallow_too_small() {
            parse("1 atto").unwrap();
            assert_eq!(
                parse("0.1 atto").unwrap_err().to_string(),
                "sub-atto amounts are not allowed"
            )
        }

        #[test]
        fn all_possible_prefixes() {
            for scale in ALL_SCALES {
                for prefix in scale.units.iter().chain([&scale.name]) {
                    // Need a space here because of the exa ambiguity
                    do_test(&format!("1 {prefix}"), "1", *scale);
                }
            }
        }
    }
}

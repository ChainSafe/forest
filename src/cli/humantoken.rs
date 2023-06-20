// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! This module defines a [parser](parse()) and
//! [pretty-printer](TokenAmountPretty::pretty) for
//! `TokenAmount`
//!
//! See the `si` module source for supported prefixes.

pub use parse::parse;
pub use print::TokenAmountPretty;

/// SI prefix definitions
mod si {
    use bigdecimal::BigDecimal;

    // Use a struct as a table row instead of an enum
    // to make our code less macro-heavy
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Prefix {
        /// `"micro"`
        pub name: &'static str,
        /// `[ "μ", "u" ]`
        pub units: &'static [&'static str],
        /// `-6`
        pub exponent: i8,
        /// `"0.000001"`
        pub multiplier: &'static str,
    }

    impl Prefix {
        // ENHANCE(aatifsyed): could memoize this if it's called in a hot loop
        pub fn multiplier(&self) -> BigDecimal {
            self.multiplier.parse().unwrap()
        }
    }

    /// Biggest first
    macro_rules! define_prefixes {
        ($($name:ident $symbol:ident$(or $alt_symbol:ident)* $base_10:literal $decimal:literal),* $(,)?) =>
            {
                // Define constants
                $(
                    #[allow(non_upper_case_globals)]
                    pub const $name: Prefix = Prefix {
                        name: stringify!($name),
                        units: &[stringify!($symbol) $(, stringify!($alt_symbol))* ],
                        exponent: $base_10,
                        multiplier: stringify!($decimal),
                    };
                )*

                /// Biggest first
                // Define top level array
                pub const SUPPORTED_PREFIXES: &[Prefix] =
                    &[
                        $(
                            $name
                        ,)*
                    ];
            };
    }

    define_prefixes! {
        quetta  Q     30     1000000000000000000000000000000,
        ronna   R     27     1000000000000000000000000000,
        yotta   Y     24     1000000000000000000000000,
        zetta   Z     21     1000000000000000000000,
        exa     E     18     1000000000000000000,
        peta    P     15     1000000000000000,
        tera    T     12     1000000000000,
        giga    G     9      1000000000,
        mega    M     6      1000000,
        kilo    k     3      1000,
        // Leave this out because
        // - it simplifies our printing logic
        // - these are not commonly used
        // - it's more consistent with lotus
        //
        // hecto    h     2     100,
        // deca     da    1     10,
        // deci     d    -1     0.1,
        // centi    c    -2     0.01,
        milli   m      -3    0.001,
        micro   μ or u -6    0.000001,
        nano    n      -9    0.000000001,
        pico    p      -12   0.000000000001,
        femto   f      -15   0.000000000000001,
        atto    a      -18   0.000000000000000001,
        zepto   z      -21   0.000000000000000000001,
        yocto   y      -24   0.000000000000000000000001,
        ronto   r      -27   0.000000000000000000000000001,
        quecto  q      -30   0.000000000000000000000000000001,
    }

    #[test]
    fn sorted() {
        let is_sorted_biggest_first = SUPPORTED_PREFIXES
            .windows(2)
            .all(|pair| pair[0].multiplier() > pair[1].multiplier());
        assert!(is_sorted_biggest_first)
    }
}

mod parse {
    // ENHANCE(aatifsyed): could accept pairs like "1 nano 1 atto"

    use crate::shim::econ::TokenAmount;
    use anyhow::{anyhow, bail};
    use bigdecimal::{BigDecimal, ParseBigDecimalError};
    use nom::{
        bytes::complete::tag,
        character::complete::multispace0,
        combinator::{map_res, opt},
        error::{FromExternalError, ParseError},
        number::complete::recognize_float,
        sequence::terminated,
        IResult,
    };

    use super::si;

    /// Parse token amounts as floats with SI prefixed-units.
    /// ```
    /// fn assert_attos(input: &str, attos: u64) {
    ///     let expected = forest_filecoin::shim::econ::TokenAmount::from_atto(attos);
    ///     let actual = forest_filecoin::cli::humantoken::parse(input).unwrap();
    ///     assert_eq!(expected, actual);
    /// }
    /// assert_attos("1a", 1);
    /// assert_attos("1aFIL", 1);
    /// assert_attos("1 femtoFIL", 1000);
    /// assert_attos("1.1 f", 1100);
    /// assert_attos("1.0e3 attofil", 1000);
    /// ```
    ///
    /// # Known bugs
    /// - `1efil` will not parse as an exa (`10^18`), because we'll try and
    ///   parse it as a exponent in the float. Instead use `1 efil`.
    pub fn parse(input: &str) -> anyhow::Result<TokenAmount> {
        let (mut big_decimal, scale) = parse_big_decimal_and_scale(input)?;

        if let Some(scale) = scale {
            big_decimal *= scale.multiplier();
        }

        let fil = big_decimal;
        let attos = fil * si::atto.multiplier().inverse();

        if !attos.is_integer() {
            bail!("sub-atto amounts are not allowed");
        }

        let (attos, scale) = attos.with_scale(0).into_bigint_and_exponent();
        assert_eq!(scale, 0, "we've just set the scale!");

        Ok(TokenAmount::from_atto(attos))
    }

    fn nom2anyhow(e: nom::Err<nom::error::VerboseError<&str>>) -> anyhow::Error {
        anyhow!("parse error: {e}")
    }

    fn parse_big_decimal_and_scale(
        input: &str,
    ) -> anyhow::Result<(BigDecimal, Option<si::Prefix>)> {
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

    /// Take an [si::Prefix] from the front of `input`
    fn si_scale<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&str, si::Prefix, E> {
        // Try the longest matches first, so we don't e.g match `a` instead of `atto`,
        // leaving `tto`.

        let mut scales = si::SUPPORTED_PREFIXES
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
        use std::str::FromStr as _;

        use num::{BigInt, One as _};

        use super::*;

        #[test]
        fn cover_scales() {
            for scale in si::SUPPORTED_PREFIXES {
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

        fn test_dec_scale(
            input: &str,
            expected_amount: &str,
            expected_scale: impl Into<Option<si::Prefix>>,
        ) {
            let expected_amount = BigDecimal::from_str(expected_amount).unwrap();
            let expected_scale = expected_scale.into();
            let (actual_amount, actual_scale) = parse_big_decimal_and_scale(input).unwrap();
            assert_eq!(expected_amount, actual_amount, "{input}");
            assert_eq!(expected_scale, actual_scale, "{input}");
        }

        #[test]
        fn basic_bigdecimal_and_scale() {
            // plain
            test_dec_scale("1", "1", None);

            // include unit
            test_dec_scale("1 FIL", "1", None);
            test_dec_scale("1FIL", "1", None);
            test_dec_scale("1 fil", "1", None);
            test_dec_scale("1fil", "1", None);

            let possible_units = ["", "fil", "FIL", " fil", " FIL"];
            let possible_prefixes = ["atto", "a", " atto", " a"];

            for unit in possible_units {
                for prefix in possible_prefixes {
                    let input = format!("1{prefix}{unit}");
                    test_dec_scale(&input, "1", si::atto)
                }
            }
        }

        #[test]
        fn parse_exa_and_exponent() {
            test_dec_scale("1 E", "1", si::exa);
            test_dec_scale("1e0E", "1", si::exa);

            // ENHANCE(aatifsyed): this should be parsed as 1 exa, but that
            // would probably require an entirely custom float parser with
            // lookahead - users will have to include a space for now

            // do_test("1E", "1", exa);
        }

        #[test]
        fn more_than_96_bits() {
            use std::iter::{once, repeat};

            // The previous rust_decimal implementation had at most 96 bits of precision
            // we should be able to exceed that
            let test_str = once('1')
                .chain(repeat('0').take(98))
                .chain(['1'])
                .collect::<String>();
            test_dec_scale(&test_str, &test_str, None);
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
        fn some_values() {
            let one_atto = TokenAmount::from_atto(BigInt::one());
            let one_nano = TokenAmount::from_nano(BigInt::one());

            assert_eq!(one_atto, parse("1 atto").unwrap());
            assert_eq!(one_atto, parse("1000 zepto").unwrap());

            assert_eq!(one_nano, parse("1 nano").unwrap());
        }

        #[test]
        fn all_possible_prefixes() {
            for scale in si::SUPPORTED_PREFIXES {
                for prefix in scale.units.iter().chain([&scale.name]) {
                    // Need a space here because of the exa ambiguity
                    test_dec_scale(&format!("1 {prefix}"), "1", *scale);
                }
            }
        }
    }
}

mod print {
    use std::fmt;

    use crate::shim::econ::TokenAmount;
    use bigdecimal::BigDecimal;
    use num::{BigInt, Zero as _};

    use super::si;

    fn scale(n: BigDecimal) -> (BigDecimal, Option<si::Prefix>) {
        for prefix in si::SUPPORTED_PREFIXES
            .iter()
            .filter(|prefix| prefix.exponent > 0)
        {
            let scaled = n.clone() / prefix.multiplier();
            if scaled.is_integer() {
                return (scaled, Some(*prefix));
            }
        }

        if n.is_integer() {
            return (n, None);
        }

        for prefix in si::SUPPORTED_PREFIXES
            .iter()
            .filter(|prefix| prefix.exponent < 0)
        {
            let scaled = n.clone() / prefix.multiplier();
            if scaled.is_integer() {
                return (scaled, Some(*prefix));
            }
        }

        let smallest_prefix = si::SUPPORTED_PREFIXES.last().unwrap();
        (n / smallest_prefix.multiplier(), Some(*smallest_prefix))
    }

    pub struct Pretty {
        attos: BigInt,
    }

    impl From<&TokenAmount> for Pretty {
        fn from(value: &TokenAmount) -> Self {
            Self {
                attos: value.atto().clone(),
            }
        }
    }

    pub trait TokenAmountPretty {
        fn pretty(&self) -> Pretty;
    }

    impl TokenAmountPretty for TokenAmount {
        /// Note the following format specifiers:
        /// - `{:#}`: print number of FIL, not e.g `milliFIL`
        /// - `{:.4}`: round to 4 significant figures
        /// - `{:.#4}`: both
        ///
        /// ```
        /// use forest_filecoin::cli::humantoken::TokenAmountPretty as _;
        ///
        /// let amount = forest_filecoin::shim::econ::TokenAmount::from_nano(1500);
        ///
        /// // Defaults to precise, with SI prefix
        /// assert_eq!("1500 nanoFIL", format!("{}", amount.pretty()));
        ///
        /// // Rounded to 1 s.f
        /// assert_eq!("~2 microFIL", format!("{:.1}", amount.pretty()));
        ///
        /// // Show absolute FIL
        /// assert_eq!("0.0000015 FIL", format!("{:#}", amount.pretty()));
        ///
        /// // Rounded absolute FIL
        /// assert_eq!("~0.000002 FIL", format!("{:#.1}", amount.pretty()));
        ///
        /// // We only indicate lost precision when relevant
        /// assert_eq!("1500 nanoFIL", format!("{:.2}", amount.pretty()));
        /// ```
        ///
        /// # Formatting
        /// - We select the most diminutive SI prefix (or not!) that allows us
        ///   to display an integer amount.
        // RUST(aatifsyed): this should be -> impl fmt::Display
        //
        // Users shouldn't be able to name `Pretty` anyway
        fn pretty(&self) -> Pretty {
            Pretty::from(self)
        }
    }

    impl fmt::Display for Pretty {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let actual_fil = &self.attos * si::atto.multiplier();

            // rounding
            let fil_for_printing = match f.precision() {
                None => actual_fil.normalized(),
                Some(prec) => actual_fil
                    .with_prec(u64::try_from(prec).expect("requested precision is absurd"))
                    .normalized(),
            };

            let precision_was_lost = fil_for_printing != actual_fil;

            if precision_was_lost {
                f.write_str("~")?;
            }

            // units or whole
            let (print_me, prefix) = match f.alternate() {
                true => (fil_for_printing, None),
                false => scale(fil_for_printing),
            };

            // write the string
            match print_me.is_zero() {
                true => f.write_str("0 FIL"),
                false => match prefix {
                    Some(prefix) => f.write_fmt(format_args!("{print_me} {}FIL", prefix.name)),
                    None => f.write_fmt(format_args!("{print_me} FIL")),
                },
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use std::str::FromStr as _;

        use num::One as _;
        use pretty_assertions::assert_eq;

        use super::*;

        #[test]
        fn prefixes_represent_themselves() {
            for prefix in si::SUPPORTED_PREFIXES {
                let input = BigDecimal::from_str(prefix.multiplier).unwrap();
                assert_eq!((BigDecimal::one(), Some(*prefix)), scale(input));
            }
        }

        #[test]
        fn very_large() {
            let mut one_thousand_quettas = String::from(si::quetta.multiplier);
            one_thousand_quettas.push_str("000");

            test_scale(&one_thousand_quettas, "1000", si::quetta);
        }

        #[test]
        fn very_small() {
            let mut one_thousanth_of_a_quecto = String::from(si::quecto.multiplier);
            one_thousanth_of_a_quecto.pop();
            one_thousanth_of_a_quecto.push_str("0001");

            test_scale(&one_thousanth_of_a_quecto, "0.001", si::quecto);
        }

        fn test_scale(
            input: &str,
            expected_value: &str,
            expected_prefix: impl Into<Option<si::Prefix>>,
        ) {
            let input = BigDecimal::from_str(input).unwrap();
            let expected_value = BigDecimal::from_str(expected_value).unwrap();
            let expected_prefix = expected_prefix.into();

            assert_eq!((expected_value, expected_prefix), scale(input))
        }

        #[test]
        fn simple() {
            test_scale("1000000", "1", si::mega);
            test_scale("100000", "100", si::kilo);
            test_scale("10000", "10", si::kilo);
            test_scale("1000", "1", si::kilo);
            test_scale("100", "100", None);
            test_scale("10", "10", None);
            test_scale("1", "1", None);
            test_scale("0.1", "100", si::milli);
            test_scale("0.01", "10", si::milli);
            test_scale("0.001", "1", si::milli);
            test_scale("0.0001", "100", si::micro);
        }
        #[test]
        fn trailing_one() {
            test_scale("10001000", "10001", si::kilo);
            test_scale("10001", "10001", None);
            test_scale("1000.1", "1000100", si::milli);
        }

        fn attos(input: &str) -> TokenAmount {
            TokenAmount::from_atto(BigInt::from_str(input).unwrap())
        }

        fn fils(input: &str) -> TokenAmount {
            TokenAmount::from_whole(BigInt::from_str(input).unwrap())
        }

        #[test]
        fn test_display() {
            assert_eq!("0 FIL", format!("{}", attos("0").pretty()));

            // Absolute works
            assert_eq!("1 attoFIL", format!("{}", attos("1").pretty()));
            assert_eq!(
                "0.000000000000000001 FIL",
                format!("{:#}", attos("1").pretty())
            );

            // We select the right suffix
            assert_eq!("1 femtoFIL", format!("{}", attos("1000").pretty()));
            assert_eq!("1001 attoFIL", format!("{}", attos("1001").pretty()));

            // If you ask for 0 precision, you get it
            assert_eq!("~0 FIL", format!("{:.0}", attos("1001").pretty()));

            // Rounding without a prefix
            assert_eq!("~10 FIL", format!("{:.1}", fils("11").pretty()));

            // Rounding with absolute
            assert_eq!(
                "~0.000000000000002 FIL",
                format!("{:#.1}", attos("1940").pretty())
            );
            assert_eq!(
                "~0.0000000000000019 FIL",
                format!("{:#.2}", attos("1940").pretty())
            );
            assert_eq!(
                "0.00000000000000194 FIL",
                format!("{:#.3}", attos("1940").pretty())
            );

            // Small numbers with a gap then a trailing one are rounded down
            assert_eq!("~1 femtoFIL", format!("{:.1}", attos("1001").pretty()));
            assert_eq!("~1 femtoFIL", format!("{:.2}", attos("1001").pretty()));
            assert_eq!("~1 femtoFIL", format!("{:.3}", attos("1001").pretty()));
            assert_eq!("1001 attoFIL", format!("{:.4}", attos("1001").pretty()));
            assert_eq!("1001 attoFIL", format!("{:.5}", attos("1001").pretty()));

            // Small numbers with trailing numbers are rounded down
            assert_eq!("~1 femtoFIL", format!("{:.1}", attos("1234").pretty()));
            assert_eq!("~1200 attoFIL", format!("{:.2}", attos("1234").pretty()));
            assert_eq!("~1230 attoFIL", format!("{:.3}", attos("1234").pretty()));
            assert_eq!("1234 attoFIL", format!("{:.4}", attos("1234").pretty()));
            assert_eq!("1234 attoFIL", format!("{:.5}", attos("1234").pretty()));

            // Small numbers are rounded appropriately
            assert_eq!("~2 femtoFIL", format!("{:.1}", attos("1900").pretty()));
            assert_eq!("~2 femtoFIL", format!("{:.1}", attos("1500").pretty()));
            assert_eq!("~1 femtoFIL", format!("{:.1}", attos("1400").pretty()));

            // Big numbers with a gap then a trailing one are rounded down
            assert_eq!("~1 kiloFIL", format!("{:.1}", fils("1001").pretty()));
            assert_eq!("~1 kiloFIL", format!("{:.2}", fils("1001").pretty()));
            assert_eq!("~1 kiloFIL", format!("{:.3}", fils("1001").pretty()));
            assert_eq!("1001 FIL", format!("{:.4}", fils("1001").pretty()));
            assert_eq!("1001 FIL", format!("{:.5}", fils("1001").pretty()));

            // Big numbers with trailing numbers are rounded down
            assert_eq!("~1 kiloFIL", format!("{:.1}", fils("1234").pretty()));
            assert_eq!("~1200 FIL", format!("{:.2}", fils("1234").pretty()));
            assert_eq!("~1230 FIL", format!("{:.3}", fils("1234").pretty()));
            assert_eq!("1234 FIL", format!("{:.4}", fils("1234").pretty()));
            assert_eq!("1234 FIL", format!("{:.5}", fils("1234").pretty()));

            // Big numbers are rounded appropriately
            assert_eq!("~2 kiloFIL", format!("{:.1}", fils("1900").pretty()));
            assert_eq!("~2 kiloFIL", format!("{:.1}", fils("1500").pretty()));
            assert_eq!("~1 kiloFIL", format!("{:.1}", fils("1400").pretty()));
        }
    }
}

#[cfg(test)]
mod fuzz {
    use quickcheck::quickcheck;

    use super::*;

    quickcheck! {
        fn roundtrip(expected: crate::shim::econ::TokenAmount) -> () {
            // Default formatting
            let actual = parse(&format!("{}", expected.pretty())).unwrap();
            assert_eq!(expected, actual);

            // Absolute formatting
            let actual = parse(&format!("{:#}", expected.pretty())).unwrap();
            assert_eq!(expected, actual);

            // Don't test rounded formatting...
        }
    }

    quickcheck! {
        fn parser_no_panic(s: String) -> () {
            let _ = parse(&s);
        }
    }
}

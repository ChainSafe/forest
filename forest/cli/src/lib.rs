// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
pub mod cli;

mod util {
    use std::str::FromStr;

    use anyhow::{anyhow, bail};
    use bigdecimal::BigDecimal;
    use fvm_shared::econ::TokenAmount;
    use nom::{Finish, IResult};
    use num::{BigInt, One as _};
    use once_cell::sync::Lazy;

    // Use a struct as a table row instead of an enum
    // to make our code less macro-heavy
    #[derive(Debug, Clone, Copy)]
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

    /// Spread the scale from tabular form to the structs, comma separated
    macro_rules! scale_array {
        ($($name:ident $symbol:ident$(or $alt_symbol:ident)* $base_10:literal $decimal:literal),* $(,)?) => {
            [$(
                SIScale {
                    name: stringify!($name),
                    units: &[stringify!($symbol) $(, stringify!($alt_symbol))* ],
                    exponent: $base_10,
                    multiplier: stringify!($decimal),
                },
            )*]
        };
    }

    const SCALES: [SIScale; 20] =
        // Lightly altered
        // https://en.wikipedia.org/wiki/Metric_prefix#List_of_SI_prefixes
        scale_array! {
        quetta	Q	30	1000000000000000000000000000000,
        ronna	R	27	1000000000000000000000000000,
        yotta	Y	24	1000000000000000000000000,
        zetta	Z	21	1000000000000000000000,
        exa	E	18	1000000000000000000,
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
        // we don't support subdivisions of an atto for our usecase
        };

    const ATTO: SIScale = get_scale("atto");

    const fn get_scale(name: &str) -> SIScale {
        let mut i = SCALES.len();
        while let Some(new_i) = i.checked_sub(1) {
            i = new_i;
            if eq(SCALES[i].name.as_bytes(), name.as_bytes()) {
                return SCALES[i];
            }
        }
        panic!("No scale with that name")
    }

    const fn eq(lhs: &[u8], rhs: &[u8]) -> bool {
        if lhs.len() != rhs.len() {
            return false;
        }
        let mut i = lhs.len();
        while let Some(new_i) = i.checked_sub(1) {
            i = new_i;
            if lhs[i] != rhs[i] {
                return false;
            }
        }
        true
    }

    fn parse_token_amount(input: &str) -> anyhow::Result<TokenAmount> {
        let (remainder, (mut big_decimal, scale)) =
            nom::combinator::all_consuming::<_, _, nom::error::Error<_>, _>(
                parse_bigdecimal_scale_maybe_unit,
            )(input)
            .map_err(|e| anyhow!("parse error: {e}"))?;
        assert!(remainder.is_empty());

        if let Some(scale) = scale {
            big_decimal *= scale.multiplier();
        }

        let fil = big_decimal;
        let attos = fil * &*ATTOS_PER_FIL;

        if !attos.is_integer() {
            bail!("sub-atto amounts are not allowed");
        }

        let (attos, scale) = attos.with_scale(0).into_bigint_and_exponent();
        assert_eq!(scale, 0, "we've just set the scale!");

        Ok(TokenAmount::from_atto(attos))
    }

    fn parse_bigdecimal_scale_maybe_unit<'a, E: nom::error::ParseError<&'a str>>(
        input: &'a str,
    ) -> IResult<&str, (BigDecimal, Option<SIScale>), E>
    where
        E: nom::error::FromExternalError<&'a str, bigdecimal::ParseBigDecimalError>,
    {
        use nom::{branch::alt, bytes::streaming::tag, combinator::opt};
        let (input, decimal) = permit_trailing_ws(bigdecimal)(input)?;
        let (input, si_scale) = permit_trailing_ws(opt(si_scale))(input)?;
        let (input, _unit) = opt(alt((tag("FIL"), tag("fil"))))(input)?;
        Ok((input, (decimal, si_scale)))
    }

    fn permit_trailing_ws<'a, F, O, E: nom::error::ParseError<&'a str>>(
        inner: F,
    ) -> impl FnMut(&'a str) -> IResult<&'a str, O, E>
    where
        F: FnMut(&'a str) -> IResult<&'a str, O, E>,
    {
        use nom::{character::streaming::multispace0, sequence::terminated};
        terminated(inner, multispace0)
    }

    /// Take an SIScale from the front of `input`
    fn si_scale<'a, E: nom::error::ParseError<&'a str>>(
        input: &'a str,
    ) -> IResult<&str, SIScale, E> {
        use nom::bytes::streaming::tag;
        for scale in SCALES {
            for prefix in scale.units.iter().chain(&[scale.name]) {
                if let Ok((rem, _prefix)) = tag::<_, _, E>(*prefix)(input) {
                    return Ok((rem, scale));
                }
            }
        }
        Err(nom::Err::Failure(E::from_error_kind(
            input,
            nom::error::ErrorKind::Alt,
        )))
    }

    /// Take a float from the front of `input`
    fn bigdecimal<'a, E: nom::error::ParseError<&'a str>>(
        input: &'a str,
    ) -> IResult<&str, BigDecimal, E>
    where
        E: nom::error::FromExternalError<&'a str, bigdecimal::ParseBigDecimalError>,
    {
        use nom::{combinator::map_res, number::streaming::recognize_float};
        map_res(recognize_float, str::parse)(input)
    }

    macro_rules! supported_prefix {
        ($($name:ident $symbol:ident $base_10:literal $decimal:literal),* $(,)?) => {
            #[allow(non_camel_case_types)]
            pub enum SupportedPrefix {
                $($name,)*
            }
            impl SupportedPrefix {
                const fn all() -> &'static [Self] {
                    &[
                        $(Self::$name,)*
                    ]
                }
                pub const fn name(&self) -> &'static str {
                    match self {
                        $(Self::$name => stringify!($name),)*
                    }
                }
                pub const fn symbol(&self) -> &'static str {
                    match self {
                        $(Self::$name => stringify!($symbol),)*
                    }
                }
                pub const fn exponent(&self) -> i8 {
                    match self {
                        $(Self::$name => $base_10,)*
                    }
                }
                pub fn multiplier(&self) -> &'static BigDecimal {
                    $(
                        #[allow(non_upper_case_globals)]
                        static $name: Lazy<BigDecimal> = Lazy::new(||BigDecimal::from_str(stringify!($decimal)).unwrap());
                    )*

                    match self {
                        $(Self::$name => &$name,)*
                    }
                }
            }
            impl FromStr for SupportedPrefix {
                type Err = anyhow::Error;
                fn from_str(s: &str) -> Result<Self, Self::Err> {
                    match s {
                        "u" => Ok(Self::micro),
                        $(
                            stringify!($name) | stringify!($symbol) => Ok(Self::$name),
                        )*
                        other => Err(
                            anyhow::anyhow!("invalid unit {other}").context(
                                concat!("expected one of: ",
                                    $(concat!(stringify!($name), " (", stringify!($symbol),") "),)*
                                )
                            )
                        )
                    }
                }
            }
        };
    }

    static ATTOS_PER_FIL: Lazy<BigDecimal> =
        Lazy::new(|| BigDecimal::from(BigInt::one() * 10u64.pow(18)));

    // Lightly altered
    // https://en.wikipedia.org/wiki/Metric_prefix#List_of_SI_prefixes
    supported_prefix!(
    quetta	Q	30	1000000000000000000000000000000,
    ronna	R	27	1000000000000000000000000000,
    yotta	Y	24	1000000000000000000000000,
    zetta	Z	21	1000000000000000000000,
    exa	E	18	1000000000000000000,
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
    micro	μ	-6	0.000001,
    nano	n	-9	0.000000001,
    pico	p	-12	0.000000000001,
    femto	f	-15	0.000000000000001,
    atto	a	-18	0.000000000000000001,
    // we don't support subdivisions of an atto for our usecase
    );

    pub fn bigdecimal_fil_to_attos(
        mut big_decimal: BigDecimal,
        prefix: Option<SupportedPrefix>,
    ) -> Option<TokenAmount> {
        if let Some(prefix) = prefix {
            big_decimal *= prefix.multiplier();
        }

        let fil = big_decimal;
        let attos = fil * &*ATTOS_PER_FIL;

        if !attos.is_integer() {
            return None;
        }

        let (attos, scale) = attos.with_scale(0).into_bigint_and_exponent();
        assert_eq!(scale, 0, "we've just set the scale!");

        Some(TokenAmount::from_atto(attos))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Catch panics in memoized scales
        #[test]
        fn cover_scales() {
            for prefix in SupportedPrefix::all() {
                let _ = prefix.multiplier();
            }
        }

        #[test]
        fn attos_per_fil() {
            assert_eq!(SupportedPrefix::atto.multiplier().inverse(), *ATTOS_PER_FIL);
            assert_eq!(
                SupportedPrefix::atto.multiplier() * &*ATTOS_PER_FIL,
                BigDecimal::one()
            );
        }
    }
}

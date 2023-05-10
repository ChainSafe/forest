// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
pub mod cli;

mod util {
    use std::str::FromStr;

    use bigdecimal::BigDecimal;
    use num::{BigInt, One as _};
    use once_cell::sync::Lazy;

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
                pub fn scale(&self) -> &'static BigDecimal {

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

    pub fn bigdecimal_to_attos(
        mut big_decimal: BigDecimal,
        prefix: Option<SupportedPrefix>,
    ) -> BigInt {
        if let Some(prefix) = prefix {
            big_decimal *= prefix.scale();
        }

        let fil = big_decimal;
        let attos = fil * &*ATTOS_PER_FIL;
        attos.with_scale(todo!());
        todo!()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        /// Catch panics in memoized scales
        #[test]
        fn cover_scales() {
            for prefix in SupportedPrefix::all() {
                let _ = prefix.scale();
            }
        }

        #[test]
        fn attos_per_fil() {
            assert_eq!(SupportedPrefix::atto.scale().inverse(), *ATTOS_PER_FIL);
            assert_eq!(
                SupportedPrefix::atto.scale() * &*ATTOS_PER_FIL,
                BigDecimal::one()
            );
        }
    }
}

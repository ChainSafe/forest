// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::math::{poly_parse, poly_val, PRECISION};
use num_bigint::{BigInt, Integer};

lazy_static! {
    static ref EXP_NUM_COEF: Vec<BigInt> = poly_parse(&[
        "-648770010757830093818553637600",
        "67469480939593786226847644286976",
        "-3197587544499098424029388939001856",
        "89244641121992890118377641805348864",
        "-1579656163641440567800982336819953664",
        "17685496037279256458459817590917169152",
        "-115682590513835356866803355398940131328",
        "340282366920938463463374607431768211456",
    ])
    .unwrap();
    static ref EXP_DENO_COEF: Vec<BigInt> = poly_parse(&[
        "1225524182432722209606361",
        "114095592300906098243859450",
        "5665570424063336070530214243",
        "194450132448609991765137938448",
        "5068267641632683791026134915072",
        "104716890604972796896895427629056",
        "1748338658439454459487681798864896",
        "23704654329841312470660182937960448",
        "259380097567996910282699886670381056",
        "2250336698853390384720606936038375424",
        "14978272436876548034486263159246028800",
        "72144088983913131323343765784380833792",
        "224599776407103106596571252037123047424",
        "340282366920938463463374607431768211456",
    ])
    .unwrap();
}

/// expneg accepts x in Q.128 format and computes e^-x.
/// It is most precise within [0, 1.725) range, where error is less than 3.4e-30.
/// Over the [0, 5) range its error is less than 4.6e-15.
/// Output is in Q.128 format.
pub(crate) fn expneg(x: &BigInt) -> BigInt {
    let num = poly_val(&EXP_NUM_COEF, x);
    let deno = poly_val(&EXP_DENO_COEF, x);

    (num << PRECISION).div_floor(&deno)
}

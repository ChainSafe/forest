// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_math::parse;
use num_bigint::BigInt;

lazy_static! {
    pub static ref NUM: [&'static str ; 7] = [
        "261417938209272870992496419296200268025",
        "7266615505142943436908456158054846846897",
        "32458783941900493142649393804518050491988",
        "17078670566130897220338060387082146864806",
        "-35150353308172866634071793531642638290419",
        "-20351202052858059355702509232125230498980",
        "-1563932590352680681114104005183375350999",
    ];

    pub static ref DENOM: [&'static str ; 7] = [
        "49928077726659937662124949977867279384",
        "2508163877009111928787629628566491583994",
        "21757751789594546643737445330202599887121",
        "53400635271583923415775576342898617051826",
        "41248834748603606604000911015235164348839",
        "9015227820322455780436733526367238305537",
        "340282366920938463463374607431768211456",
    ];

    pub static ref CONST_STRS: [&'static str ; 4] = [
        "314760000000000000000000000000000000",    // DefaultAlpha
        "96640100000000000000000000000000",        // DefaultBeta
        "302231454903657293676544",                // Epsilon
        "235865763225513294137944142764154484399", // ln(2)
    ];

}

const EXTRAPOLATED_CUM_SUM_RATIO_EPSILON: usize = 2;
const LN_2: usize = 3;

pub fn get_ln_num_coef() -> Vec<BigInt> {
    parse(&NUM[..]).unwrap()
}

pub fn get_ln_denom_coef() -> Vec<BigInt> {
    parse(&DENOM[..]).unwrap()
}

pub fn get_ln2() -> BigInt {
    BigInt::parse_bytes(CONST_STRS[LN_2].as_bytes(), 10).unwrap()
}

pub fn get_cum_sum_ratio_epsilon() -> BigInt {
    BigInt::parse_bytes(
        CONST_STRS[EXTRAPOLATED_CUM_SUM_RATIO_EPSILON].as_bytes(),
        10,
    )
    .unwrap()
}

#[test]
fn parse_check() {
    let _ = get_ln_num_coef();
    let _ = get_ln_denom_coef();
}

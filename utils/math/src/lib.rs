// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use num_bigint::BigInt;

pub const PRECISION: u64 = 128;

pub fn poly_val(poly: &[BigInt], x: &BigInt) -> BigInt {
    let mut res = BigInt::default();

    for coeff in poly {
        res = ((res * x) >> PRECISION) + coeff;
    }
    res
}

pub fn parse(coefs: &[&str]) -> Result<Vec<BigInt>, ()> {
    println!("Num elements is {}", coefs.len());

    let mut out: Vec<BigInt> = Vec::with_capacity(coefs.len() as usize);

    for coef in coefs {
        let c = BigInt::parse_bytes(coef.as_bytes(), 10).ok_or(())?;
        out.push(c);
    }
    Ok(out)
}

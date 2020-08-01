
use std::error:: Error;





use num_bigint::{
    biguint_ser::{BigUintDe, BigUintSer},
    BigInt, BigUint,
};

pub const PRECISION : u64 = 128;

pub fn poly_val(poly : &[BigInt], x : &BigInt ) -> BigInt {

    let mut res = BigInt::default();

    for coeff in poly {
        let temp = &res *  x;
        res = (temp >> PRECISION) + coeff;
    }
    res
}


pub fn parse(coefs : &[&str]) -> Result<Vec<BigInt>, ()>  {

    println!("Num elements is {}", coefs.len());

    let mut out : Vec<BigInt> = Vec::with_capacity(coefs.len() as usize);

    for (i, coef) in coefs.iter().enumerate() {
        let c = BigInt::parse_bytes(coef.as_bytes() , 10).ok_or(())?  ;
        out.push(c);
    }
    Ok(out)
}






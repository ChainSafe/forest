use cid::Cid;
use num_bigint::BigUint;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Actor {
    code_cid: Cid,
    state: Cid,
    balance: BigUint,
    sequence: u64,
}

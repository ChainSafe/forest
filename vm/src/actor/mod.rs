mod code;

pub use self::code::*;

use cid::Cid;
use num_bigint::BigUint;

/// State of all actor implementations
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ActorState {
    code_id: CodeID,
    state: Cid,
    balance: BigUint,
    sequence: u64,
}

/// Actor trait which defines the common functionality of system Actors
pub trait Actor {
    /// Returns Actor Cid
    fn cid(&self) -> Cid;
    /// Actor public key, if it exists
    fn public_key(&self) -> Vec<u8>;
}

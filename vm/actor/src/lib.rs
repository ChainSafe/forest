mod builtin;
mod code;

pub use self::builtin::*;
pub use self::code::*;

use cid::Cid;
use encoding::{Cbor, CodecProtocol, Error as EncodingError};
use num_bigint::BigUint;

#[derive(PartialEq, Eq, Copy, Clone, Debug, Default)]
pub struct ActorID(u64);

impl Cbor for ActorID {
    fn unmarshal_cbor(_bz: &[u8]) -> Result<Self, EncodingError> {
        // TODO
        Err(EncodingError::Unmarshalling {
            description: "Not Implemented".to_string(),
            protocol: CodecProtocol::Cbor,
        })
    }
    fn marshal_cbor(&self) -> Result<Vec<u8>, EncodingError> {
        // TODO
        Err(EncodingError::Marshalling {
            description: format!("Not implemented, data: {:?}", self),
            protocol: CodecProtocol::Cbor,
        })
    }
}

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

use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::{serde_bytes, Cbor};

#[derive(Debug, Serialize_tuple, Deserialize_tuple)]
pub struct AuthenticateMessageParams {
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub message: Vec<u8>,
}

impl Cbor for AuthenticateMessageParams {}

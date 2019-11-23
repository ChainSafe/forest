use super::signature::Signature;
use crate::address::Address;
use std::error::Error;

/// Signer is a trait which allows a key implementation to sign data for an address
pub trait Signer {
    fn sign_bytes(&self, data: Vec<u8>, address: Address) -> Result<Signature, Box<dyn Error>>;
}

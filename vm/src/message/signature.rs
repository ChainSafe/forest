use crate::address::{Address, Protocol};
use blake2::digest::*;
use blake2::VarBlake2b;

/// Signature, represented in bytes, of any key protocol
pub type Signature = Vec<u8>;

/// checks if a signature is valid given data and address
pub fn is_valid_signature(data: Vec<u8>, addr: Address, sig: Signature) -> bool {
    match addr.protocol() {
        Protocol::BLS => check_bls_sig(data, addr, sig),
        Protocol::Secp256k1 => check_secp256k1_sig(data, addr, sig),
        _ => false,
    }
}

/// returns true if a bls signature is valid
fn check_bls_sig(_data: Vec<u8>, _addr: Address, _sig: Signature) -> bool {
    // verify BLS signature with addr payload, data, and signature
    false
}

/// returns true if a secp256k1 signature is valid
fn check_secp256k1_sig(data: Vec<u8>, _addr: Address, _sig: Signature) -> bool {
    // blake2b 256 hash
    let _ = blake2b_256(data);

    // Ecrecover with hash and signature
    // TODO

    // Generate address with pub key
    // let rec_addr = Address::new_secp256k1(pubk);

    // check address against address
    // addr == rec_addr
    false
}

/// generates blake2b hash of 32 bytes
fn blake2b_256(ingest: Vec<u8>) -> Vec<u8> {
    let mut hasher = VarBlake2b::new(32).unwrap();
    hasher.input(ingest);

    // allocate hash result vector
    let mut result: Vec<u8> = vec![0; 32];

    hasher.variable_result(|res| {
        // Copy result slice to vector return
        result[..32].clone_from_slice(res);
    });

    result
}

use address::{Address, Protocol};
use blake2::digest::*;
use blake2::VarBlake2b;

use secp256k1::Error;
use secp256k1::{recover, Message, RecoveryId, Signature as EcsdaSignature};

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
fn check_secp256k1_sig(data: Vec<u8>, addr: Address, sig: Signature) -> bool {
    // blake2b 256 hash
    let mut hash = [0u8; 32];
    blake2b_256(data, &mut hash);

    // Ecrecover with hash and signature
    let mut signature = [0u8; 65];
    signature[..].clone_from_slice(sig.as_ref());
    let pub_k = ecrecover(&hash, &signature);

    // Generate address with pubkey
    let addr_res = if let Ok(key) = pub_k {
        Address::new_secp256k1(key)
    } else {
        return false;
    };

    // check address against recovered address
    match addr_res {
        Ok(rec_addr) => addr == rec_addr,
        Err(_) => false,
    }
}

const HASH_LENGTH: usize = 32;

fn ecrecover(hash: &[u8; HASH_LENGTH], signature: &[u8; 65]) -> Result<Vec<u8>, Error> {
    /* Recovery id is the last big-endian byte. */
    let v = (signature[64] as i8 - 27) as u8;
    if v != 0 && v != 1 {
        return Ok(vec![0u8; 0]);
    }

    // Signature value without recovery byte
    let mut s = [0u8; 64];
    s[..64].clone_from_slice(signature.as_ref());

    // generate types to recover key from
    let message = Message::parse(&hash);
    let rec_id = RecoveryId::parse(signature[64])?;
    let sig = EcsdaSignature::parse(&s);

    let key = recover(&message, &sig, &rec_id)?;
    let ret = key.serialize();
    Ok(ret.to_vec())
}

/// generates blake2b hash of 32 bytes
fn blake2b_256(ingest: Vec<u8>, hash: &mut [u8; 32]) {
    let mut hasher = VarBlake2b::new(32).unwrap();
    hasher.input(ingest);

    hasher.variable_result(|res| {
        // Copy result slice to vector return
        hash[..32].clone_from_slice(res);
    });
}

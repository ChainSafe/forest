// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::errors::Error;
use address::{Address, Protocol};
use bls_signatures::{
    hash as bls_hash, verify, PublicKey as BlsPubKey, Serialize, Signature as BlsSignature,
};
use encoding::blake2b_256;

use secp256k1::{recover, Message, RecoveryId, Signature as EcsdaSignature};

pub const BLS_SIG_LEN: usize = 96; // bytes
pub const BLS_PUB_LEN: usize = 48; // bytes

/// A cryptographic signature, represented in bytes, of any key protocol
pub type Signature = Vec<u8>;

/// Checks if a signature is valid given data and address
pub fn is_valid_signature(data: &[u8], addr: Address, sig: Signature) -> bool {
    match addr.protocol() {
        Protocol::BLS => verify_bls_sig(data, addr.payload(), sig),
        Protocol::Secp256k1 => verify_secp256k1_sig(data, addr, sig),
        _ => false,
    }
}

/// Returns true if a bls signature is valid
pub(crate) fn verify_bls_sig(data: &[u8], pub_k: Vec<u8>, sig: Signature) -> bool {
    if pub_k.len() != BLS_PUB_LEN || sig.len() != BLS_SIG_LEN {
        // validates pubkey length and signature length for protocol
        return false;
    }

    // hash data to be verified
    let hashed = bls_hash(data);

    // generate public key object from bytes
    let pk = match BlsPubKey::from_bytes(&pub_k) {
        Ok(v) => v,
        Err(_) => return false,
    };
    // generate signature struct from bytes
    let sig = match BlsSignature::from_bytes(sig.as_ref()) {
        Ok(v) => v,
        Err(_) => return false,
    };

    // BLS verify hash against key
    verify(&sig, &[hashed], &[pk])
}

/// Returns true if a secp256k1 signature is valid
fn verify_secp256k1_sig(data: &[u8], addr: Address, sig: Signature) -> bool {
    // blake2b 256 hash
    let hash = blake2b_256(data);

    // Ecrecover with hash and signature
    let mut signature = [0u8; 65];
    signature[..].clone_from_slice(sig.as_ref());
    let rec_addr = ecrecover(&hash, &signature);

    // check address against recovered address
    match rec_addr {
        Ok(r) => addr == r,
        Err(_) => false,
    }
}

// TODO: verify signature data format after signing implemented
fn ecrecover(hash: &[u8; 32], signature: &[u8; 65]) -> Result<Address, Error> {
    /* Recovery id is the last big-endian byte. */
    let v = (signature[64] as i8 - 27) as u8;
    if v != 0 && v != 1 {
        return Err(Error::InvalidRecovery("invalid recovery byte".to_owned()));
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
    let addr = Address::new_secp256k1(ret.to_vec())?;
    Ok(addr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use address::Address;
    use bls_signatures::{PrivateKey, Serialize, Signature as BlsSignature};
    use rand::{Rng, SeedableRng, XorShiftRng};

    #[test]
    fn bls_verify() {
        let rng = &mut XorShiftRng::from_seed([0x3dbe6259, 0x8d313d76, 0x3237db17, 0xe5bc0654]);
        let sk = PrivateKey::generate(rng);

        let msg = (0..64).map(|_| rng.gen()).collect::<Vec<u8>>();
        let signature = sk.sign(&msg);

        let signature_bytes = signature.as_bytes();
        assert_eq!(signature_bytes.len(), 96);
        assert_eq!(
            BlsSignature::from_bytes(&signature_bytes).unwrap(),
            signature
        );

        let pk = sk.public_key();
        let addr = Address::new_bls(pk.as_bytes()).unwrap();

        assert_eq!(
            is_valid_signature(&msg, addr, signature_bytes.clone()),
            true
        );
        assert_eq!(
            verify_bls_sig(&msg, pk.as_bytes(), signature_bytes.clone()),
            true
        );
    }
}

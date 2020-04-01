// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use address::{Address, Protocol};
use bls_signatures::{
    hash as bls_hash, paired::bls12_381::G2, verify, PublicKey as BlsPubKey, Serialize,
    Signature as BlsSignature,
};
use encoding::{blake2b_256, de, ser, serde_bytes};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use secp256k1::{recover, Message, RecoveryId, Signature as EcsdaSignature};

/// BLS signature length in bytes
pub const BLS_SIG_LEN: usize = 96;
/// BLS Public key length in bytes
pub const BLS_PUB_LEN: usize = 48;

/// Signature variants for Forest signatures
#[derive(Clone, Debug, PartialEq, FromPrimitive, Copy)]
pub enum SignatureType {
    Secp256 = 1,
    BLS = 2,
}

// Just used for defaulting for block signatures, can be removed later if needed
impl Default for SignatureType {
    fn default() -> Self {
        SignatureType::BLS
    }
}

impl SignatureType {
    /// Allows referencing back to Protocol from encoded byte
    fn from_byte(b: u8) -> Option<SignatureType> {
        FromPrimitive::from_u8(b)
    }
}

/// A cryptographic signature, represented in bytes, of any key protocol
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Signature {
    sig_type: SignatureType,
    bytes: Vec<u8>,
}

impl ser::Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let mut bytes = self.bytes.clone();
        // Insert signature type byte
        bytes.insert(0, self.sig_type as u8);

        let value = serde_bytes::Bytes::new(&bytes);
        serde_bytes::Serialize::serialize(value, serializer)
    }
}

impl<'de> de::Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let mut bytes: Vec<u8> = serde_bytes::Deserialize::deserialize(deserializer)?;
        // Remove signature type byte
        let sig_type = SignatureType::from_byte(bytes.remove(0))
            .ok_or_else(|| de::Error::custom("Invalid signature type byte (must be 1 or 2)"))?;

        Ok(Signature { bytes, sig_type })
    }
}

impl Signature {
    /// Creates a SECP Signature given the raw bytes
    pub fn new_secp256k1(bytes: Vec<u8>) -> Self {
        Self {
            sig_type: SignatureType::Secp256,
            bytes,
        }
    }

    /// Creates a BLS Signature given the raw bytes
    pub fn new_bls(bytes: Vec<u8>) -> Self {
        Self {
            sig_type: SignatureType::BLS,
            bytes,
        }
    }

    /// Returns reference to signature bytes
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns reference to signature type
    pub fn signature_type(&self) -> SignatureType {
        self.sig_type
    }
}

/// Checks if a signature is valid given data and address
pub fn is_valid_signature(data: &[u8], addr: &Address, sig: &Signature) -> bool {
    match addr.protocol() {
        Protocol::BLS => verify_bls_sig(data, addr.payload(), sig),
        Protocol::Secp256k1 => verify_secp256k1_sig(data, addr, sig),
        _ => false,
    }
}

/// Returns true if a bls signature is valid
pub(crate) fn verify_bls_sig(data: &[u8], pub_k: &[u8], sig: &Signature) -> bool {
    if pub_k.len() != BLS_PUB_LEN || sig.bytes().len() != BLS_SIG_LEN {
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
    let sig = match BlsSignature::from_bytes(sig.bytes()) {
        Ok(v) => v,
        Err(_) => return false,
    };

    // BLS verify hash against key
    verify(&sig, &[hashed], &[pk])
}

pub fn verify_bls_aggregate(data: &[&[u8]], pub_keys: &[&[u8]], aggregate_sig: &Signature) -> bool {
    // If the number of public keys and data does not match, then return false
    if data.len() != pub_keys.len() {
        return false;
    }

    let sig = match BlsSignature::from_bytes(aggregate_sig.bytes()) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let pk_map_results: Result<Vec<_>, _> =
        pub_keys.iter().map(|x| BlsPubKey::from_bytes(x)).collect();

    let pks = match pk_map_results {
        Ok(v) => v,
        Err(_) => return false,
    };

    let hashed_data: Vec<G2> = data.iter().map(|x| bls_hash(x)).collect();

    // DOes the aggregate verification
    verify(&sig, &hashed_data[..], &pks[..])
}

/// Returns true if a secp256k1 signature is valid
fn verify_secp256k1_sig(data: &[u8], addr: &Address, sig: &Signature) -> bool {
    // blake2b 256 hash
    let hash = blake2b_256(data);

    // Ecrecover with hash and signature
    let mut signature = [0u8; 65];
    signature[..].clone_from_slice(sig.bytes());
    let rec_addr = ecrecover(&hash, &signature);

    // check address against recovered address
    match rec_addr {
        Ok(r) => addr == &r,
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
    let addr = Address::new_secp256k1(&ret)?;
    Ok(addr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use address::Address;
    use bls_signatures::{PrivateKey, Serialize, Signature as BlsSignature};
    use rand::rngs::mock::StepRng;
    use rand::Rng;

    #[test]
    fn bls_verify() {
        let rng = &mut StepRng::new(8, 3);
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
            is_valid_signature(&msg, &addr, &Signature::new_bls(signature_bytes.clone())),
            true
        );
        assert_eq!(
            verify_bls_sig(
                &msg,
                &pk.as_bytes(),
                &Signature::new_bls(signature_bytes.clone())
            ),
            true
        );
    }
    #[test]
    fn bls_agg_verify() {
        // The number of signatures in aggregate
        let num_sigs = 10;
        let message_length = num_sigs * 64;

        let rng = &mut StepRng::new(8, 3);

        let msg = (0..message_length).map(|_| rng.gen()).collect::<Vec<u8>>();
        let data: Vec<&[u8]> = (0..num_sigs).map(|x| &msg[x * 64..(x + 1) * 64]).collect();

        let private_keys: Vec<PrivateKey> =
            (0..num_sigs).map(|_| PrivateKey::generate(rng)).collect();
        let public_keys: Vec<_> = private_keys
            .iter()
            .map(|x| x.public_key().as_bytes())
            .collect();

        let signatures: Vec<BlsSignature> = (0..num_sigs)
            .map(|x| private_keys[x].sign(data[x]))
            .collect();

        let mut public_keys_slice: Vec<&[u8]> = vec![];
        for i in 0..num_sigs {
            public_keys_slice.push(&public_keys[i]);
        }

        let calculated_bls_agg =
            Signature::new_bls(bls_signatures::aggregate(&signatures).as_bytes());
        assert_eq!(
            verify_bls_aggregate(&data, &public_keys_slice, &calculated_bls_agg),
            true
        );
    }
}

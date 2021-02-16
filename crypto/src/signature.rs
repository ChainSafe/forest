// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use address::{Address, Protocol};
use bls_signatures::{
    verify_messages, PublicKey as BlsPubKey, Serialize, Signature as BlsSignature,
};
use encoding::{blake2b_256, de, repr::*, ser, serde_bytes};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use secp256k1::{recover, Message, RecoveryId, Signature as EcsdaSignature};
use std::borrow::Cow;

/// BLS signature length in bytes.
pub const BLS_SIG_LEN: usize = 96;
/// BLS Public key length in bytes.
pub const BLS_PUB_LEN: usize = 48;

/// Secp256k1 signature length in bytes.
pub const SECP_SIG_LEN: usize = 65;
/// Secp256k1 Public key length in bytes.
pub const SECP_PUB_LEN: usize = 65;

/// Signature variants for Filecoin signatures.
#[derive(
    Clone, Debug, PartialEq, FromPrimitive, Copy, Eq, Serialize_repr, Deserialize_repr, Hash,
)]
#[repr(u8)]
pub enum SignatureType {
    Secp256k1 = 1,
    BLS = 2,
}

/// A cryptographic signature, represented in bytes, of any key protocol.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Signature {
    sig_type: SignatureType,
    bytes: Vec<u8>,
}

impl ser::Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let mut bytes = Vec::with_capacity(self.bytes.len() + 1);
        // Insert signature type byte
        bytes.push(self.sig_type as u8);
        bytes.extend_from_slice(&self.bytes);

        serde_bytes::Serialize::serialize(&bytes, serializer)
    }
}

impl<'de> de::Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let bytes: Cow<'de, [u8]> = serde_bytes::Deserialize::deserialize(deserializer)?;
        if bytes.is_empty() {
            return Err(de::Error::custom("Cannot deserialize empty bytes"));
        }

        // Remove signature type byte
        let sig_type = SignatureType::from_u8(bytes[0])
            .ok_or_else(|| de::Error::custom("Invalid signature type byte (must be 1 or 2)"))?;

        Ok(Signature {
            bytes: bytes[1..].to_vec(),
            sig_type,
        })
    }
}

impl Signature {
    /// Creates a SECP Signature given the raw bytes.
    pub fn new_secp256k1(bytes: Vec<u8>) -> Self {
        Self {
            sig_type: SignatureType::Secp256k1,
            bytes,
        }
    }

    /// Creates a BLS Signature given the raw bytes.
    pub fn new_bls(bytes: Vec<u8>) -> Self {
        Self {
            sig_type: SignatureType::BLS,
            bytes,
        }
    }

    /// Returns reference to signature bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns [SignatureType] for the signature.
    pub fn signature_type(&self) -> SignatureType {
        self.sig_type
    }

    /// Checks if a signature is valid given data and address.
    pub fn verify(&self, data: &[u8], addr: &Address) -> Result<(), String> {
        match addr.protocol() {
            Protocol::BLS => verify_bls_sig(self.bytes(), data, addr),
            Protocol::Secp256k1 => verify_secp256k1_sig(self.bytes(), data, addr),
            _ => Err("Address must be resolved to verify a signature".to_owned()),
        }
    }
}

/// Returns `String` error if a bls signature is invalid.
pub(crate) fn verify_bls_sig(signature: &[u8], data: &[u8], addr: &Address) -> Result<(), String> {
    let pub_k = addr.payload_bytes();

    // generate public key object from bytes
    let pk = BlsPubKey::from_bytes(&pub_k).map_err(|e| e.to_string())?;

    // generate signature struct from bytes
    let sig = BlsSignature::from_bytes(signature).map_err(|e| e.to_string())?;

    // BLS verify hash against key
    if verify_messages(&sig, &[data], &[pk]) {
        Ok(())
    } else {
        Err(format!(
            "bls signature verification failed for addr: {}",
            addr
        ))
    }
}

/// Returns `String` error if a secp256k1 signature is invalid.
fn verify_secp256k1_sig(signature: &[u8], data: &[u8], addr: &Address) -> Result<(), String> {
    if signature.len() != SECP_SIG_LEN {
        return Err(format!(
            "Invalid Secp256k1 signature length. Was {}, must be 65",
            signature.len()
        ));
    }

    // blake2b 256 hash
    let hash = blake2b_256(data);

    // Ecrecover with hash and signature
    let mut sig = [0u8; SECP_SIG_LEN];
    sig[..].copy_from_slice(signature);
    let rec_addr = ecrecover(&hash, &sig).map_err(|e| e.to_string())?;

    // check address against recovered address
    if &rec_addr == addr {
        Ok(())
    } else {
        Err("Secp signature verification failed".to_owned())
    }
}
/// Aggregates and verifies bls signatures collectively.
pub fn verify_bls_aggregate(data: &[&[u8]], pub_keys: &[&[u8]], aggregate_sig: &Signature) -> bool {
    // If the number of public keys and data does not match, then return false
    if data.len() != pub_keys.len() {
        return false;
    }
    if data.is_empty() {
        return true;
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

    // Does the aggregate verification
    verify_messages(&sig, data, &pks[..])
}

/// Return Address for a message given it's signing bytes hash and signature.
pub fn ecrecover(hash: &[u8; 32], signature: &[u8; SECP_SIG_LEN]) -> Result<Address, Error> {
    // generate types to recover key from
    let rec_id = RecoveryId::parse(signature[64])?;
    let message = Message::parse(&hash);

    // Signature value without recovery byte
    let mut s = [0u8; 64];
    s.clone_from_slice(signature[..64].as_ref());
    // generate Signature
    let sig = EcsdaSignature::parse(&s);

    let key = recover(&message, &sig, &rec_id)?;
    let ret = key.serialize();
    let addr = Address::new_secp256k1(&ret)?;
    Ok(addr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bls_signatures::{PrivateKey, Serialize, Signature as BlsSignature};
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    use secp256k1::{sign, PublicKey, SecretKey};

    #[test]
    fn bls_agg_verify() {
        // The number of signatures in aggregate
        let num_sigs = 10;
        let message_length = num_sigs * 64;

        let rng = &mut ChaCha8Rng::seed_from_u64(11);

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
            Signature::new_bls(bls_signatures::aggregate(&signatures).unwrap().as_bytes());
        assert_eq!(
            verify_bls_aggregate(&data, &public_keys_slice, &calculated_bls_agg),
            true
        );
    }

    #[test]
    fn secp_ecrecover() {
        let rng = &mut ChaCha8Rng::seed_from_u64(8);

        let priv_key = SecretKey::random(rng);
        let pub_key = PublicKey::from_secret_key(&priv_key);
        let secp_addr = Address::new_secp256k1(&pub_key.serialize()).unwrap();

        let hash = blake2b_256(&[8, 8]);
        let msg = Message::parse(&hash);

        // Generate signature
        let (sig, recovery_id) = sign(&msg, &priv_key);
        let mut signature = [0; 65];
        signature[..64].copy_from_slice(&sig.serialize());
        signature[64] = recovery_id.serialize();

        assert_eq!(ecrecover(&hash, &signature).unwrap(), secp_addr);
    }
}

#[cfg(feature = "json")]
pub mod json {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    // Wrapper for serializing and deserializing a Signature from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct SignatureJson(#[serde(with = "self")] pub Signature);

    /// Wrapper for serializing a Signature reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct SignatureJsonRef<'a>(#[serde(with = "self")] pub &'a Signature);

    #[derive(Serialize, Deserialize)]
    struct JsonHelper {
        #[serde(rename = "Type")]
        sig_type: SignatureType,
        #[serde(rename = "Data")]
        bytes: String,
    }

    pub fn serialize<S>(m: &Signature, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            sig_type: m.sig_type,
            bytes: base64::encode(&m.bytes),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Signature, D::Error>
    where
        D: Deserializer<'de>,
    {
        let JsonHelper { sig_type, bytes } = Deserialize::deserialize(deserializer)?;
        Ok(Signature {
            sig_type,
            bytes: base64::decode(bytes).map_err(de::Error::custom)?,
        })
    }

    pub mod opt {
        use super::{Signature, SignatureJson, SignatureJsonRef};
        use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

        pub fn serialize<S>(v: &Option<Signature>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            v.as_ref()
                .map(|s| SignatureJsonRef(s))
                .serialize(serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Signature>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s: Option<SignatureJson> = Deserialize::deserialize(deserializer)?;
            Ok(s.map(|v| v.0))
        }
    }

    pub mod signature_type {
        use super::*;
        use serde::{Deserialize, Deserializer, Serialize, Serializer};

        #[derive(Debug, Deserialize, Serialize)]
        #[serde(rename_all = "lowercase")]
        enum JsonHelperEnum {
            Bls,
            Secp256k1,
        }

        #[derive(Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct SignatureTypeJson(#[serde(with = "self")] pub SignatureType);

        pub fn serialize<S>(m: &SignatureType, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let json = match m {
                SignatureType::BLS => JsonHelperEnum::Bls,
                SignatureType::Secp256k1 => JsonHelperEnum::Secp256k1,
            };
            json.serialize(serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<SignatureType, D::Error>
        where
            D: Deserializer<'de>,
        {
            let json_enum: JsonHelperEnum = Deserialize::deserialize(deserializer)?;

            let signature_type = match json_enum {
                JsonHelperEnum::Bls => SignatureType::BLS,
                JsonHelperEnum::Secp256k1 => SignatureType::Secp256k1,
            };
            Ok(signature_type)
        }
    }
}

// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
pub use super::fvm_shared_latest::{
    crypto::signature::SECP_SIG_LEN, IPLD_RAW, TICKET_RANDOMNESS_LOOKBACK,
};
use super::{
    fvm_shared_latest::{self, commcid::Commitment},
    version::NetworkVersion,
};
use crate::eth::{EthChainId, EthTx};
use crate::message::{Message, SignedMessage};
use anyhow::ensure;
use bls_signatures::{PublicKey as BlsPublicKey, Signature as BlsSignature};
use cid::Cid;
use fvm_ipld_encoding::{
    de,
    repr::{Deserialize_repr, Serialize_repr},
    ser, strict_bytes,
};
use num::FromPrimitive;
use num_derive::FromPrimitive;
use schemars::JsonSchema;
use std::borrow::Cow;

/// A cryptographic signature, represented in bytes, of any key protocol.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct Signature {
    pub sig_type: SignatureType,
    pub bytes: Vec<u8>,
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

        strict_bytes::Serialize::serialize(&bytes, serializer)
    }
}

impl<'de> de::Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let bytes: Cow<'de, [u8]> = strict_bytes::Deserialize::deserialize(deserializer)?;
        match bytes.split_first() {
            None => Err(de::Error::custom("Cannot deserialize empty bytes")),
            Some((&sig_byte, rest)) => {
                // Remove signature type byte
                let sig_type = SignatureType::from_u8(sig_byte).ok_or_else(|| {
                    de::Error::custom(format!(
                        "Invalid signature type byte (must be 1, 2 or 3), was {}",
                        sig_byte
                    ))
                })?;

                Ok(Signature {
                    bytes: rest.to_vec(),
                    sig_type,
                })
            }
        }
    }
}

impl Signature {
    pub fn new(sig_type: SignatureType, bytes: Vec<u8>) -> Self {
        Signature { sig_type, bytes }
    }

    /// Creates a BLS Signature given the raw bytes.
    pub fn new_bls(bytes: Vec<u8>) -> Self {
        Self {
            sig_type: SignatureType::Bls,
            bytes,
        }
    }

    /// Creates a SECP Signature given the raw bytes.
    pub fn new_secp256k1(bytes: Vec<u8>) -> Self {
        Self {
            sig_type: SignatureType::Secp256k1,
            bytes,
        }
    }

    /// Creates a Delegated Signature given the raw bytes.
    pub fn new_delegated(bytes: Vec<u8>) -> Self {
        Self {
            sig_type: SignatureType::Delegated,
            bytes,
        }
    }

    /// Creates a signature from bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, anyhow::Error> {
        if bytes.is_empty() {
            anyhow::bail!("Empty signature bytes");
        }

        let first_byte = bytes
            .first()
            .ok_or_else(|| anyhow::anyhow!("Invalid signature bytes"))?;

        let signature_data = bytes
            .get(1..)
            .ok_or_else(|| anyhow::anyhow!("Invalid signature bytes"))?
            .to_vec();

        // the first byte in signature represents the signature type
        let sig_type = SignatureType::try_from(*first_byte)?;
        match sig_type {
            SignatureType::Secp256k1 => Ok(Self::new_secp256k1(signature_data)),
            SignatureType::Bls => Ok(Self::new_bls(signature_data)),
            SignatureType::Delegated => Ok(Self::new_delegated(signature_data)),
        }
    }

    /// Returns the signature bytes including the signature type byte.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.bytes.len() + 1);
        bytes.push(self.sig_type as u8);
        bytes.extend_from_slice(&self.bytes);
        bytes
    }

    pub fn signature_type(&self) -> SignatureType {
        self.sig_type
    }

    /// Authenticates the message signature using protocol-specific validation:
    /// - Delegated: Uses ethereum message with RLP encoding for signature verification, Verifies message roundtrip integrity
    /// - BLS/SECP: Standard signature verification
    pub fn authenticate_msg(
        &self,
        eth_chain_id: EthChainId,
        msg: &SignedMessage,
        addr: &crate::shim::address::Address,
    ) -> anyhow::Result<()> {
        let mut sig = self.clone();
        let digest: Vec<u8>;
        match self.sig_type {
            SignatureType::Delegated => {
                let eth_tx = EthTx::from_signed_message(eth_chain_id, msg)?;
                let filecoin_msg = eth_tx.get_unsigned_message(msg.from(), eth_chain_id)?;
                ensure!(
                    msg.message().cid() == filecoin_msg.cid(),
                    "Ethereum transaction roundtrip mismatch"
                );
                // delegated uses rlp encoding for the message
                digest = eth_tx.rlp_unsigned_message(eth_chain_id)?;
                // update the exiting signature bytes with the verifiable signature for delegated signature
                sig.bytes =
                    eth_tx.to_verifiable_signature(Vec::from(self.bytes()), eth_chain_id)?;
            }
            _ => digest = msg.message().cid().to_bytes(),
        }
        sig.verify(&digest, addr)
    }

    /// Checks if a signature is valid given data and address.
    pub fn verify(&self, data: &[u8], addr: &crate::shim::address::Address) -> anyhow::Result<()> {
        use super::fvm_shared_latest::crypto::signature::ops::{
            verify_bls_sig, verify_secp256k1_sig,
        };
        match self.sig_type {
            SignatureType::Bls => {
                verify_bls_sig(&self.bytes, data, addr).map_err(anyhow::Error::msg)
            }
            SignatureType::Secp256k1 => {
                verify_secp256k1_sig(&self.bytes, data, addr).map_err(anyhow::Error::msg)
            }
            SignatureType::Delegated => verify_delegated_sig(&self.bytes, data, addr),
        }
    }

    /// Returns reference to signature bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Checks if the signature is a valid `secp256k1` signature type given the network version.
    pub fn is_valid_secpk_sig_type(&self, network_version: NetworkVersion) -> bool {
        if network_version < NetworkVersion::V18 {
            matches!(self.sig_type, SignatureType::Secp256k1)
        } else {
            matches!(
                self.sig_type,
                SignatureType::Secp256k1 | SignatureType::Delegated
            )
        }
    }
}

impl TryFrom<&Signature> for BlsSignature {
    type Error = anyhow::Error;
    fn try_from(value: &Signature) -> Result<Self, Self::Error> {
        use bls_signatures::Serialize as _;

        match value.sig_type {
            SignatureType::Secp256k1 => {
                anyhow::bail!("cannot convert Secp256k1 signature to bls signature")
            }
            SignatureType::Bls => Ok(BlsSignature::from_bytes(&value.bytes)?),
            SignatureType::Delegated => {
                anyhow::bail!("cannot convert delegated signature to bls signature")
            }
        }
    }
}

// Forest's version of the `verify_bls_aggregate` function is semantically different
// from the version in FVM.
/// Aggregates and verifies BLS signatures collectively.
pub fn verify_bls_aggregate(data: &[&[u8]], pub_keys: &[BlsPublicKey], sig: &Signature) -> bool {
    // If the number of public keys and data does not match, then return false
    if data.len() != pub_keys.len() {
        return false;
    }
    if data.is_empty() {
        return true;
    }

    let bls_sig = match sig.try_into() {
        Ok(bls_sig) => bls_sig,
        _ => return false,
    };

    // Does the aggregate verification
    bls_signatures::verify_messages(&bls_sig, data, pub_keys)
}

/// Returns `String` error if a BLS signature is invalid.
pub fn verify_bls_sig(
    signature: &[u8],
    data: &[u8],
    addr: &crate::shim::address::Address,
) -> Result<(), String> {
    fvm_shared_latest::crypto::signature::ops::verify_bls_sig(signature, data, &addr.into())
}

/// Returns `String` error if a delegated signature is invalid.
pub fn verify_delegated_sig(
    signature: &[u8],
    data: &[u8],
    addr: &crate::shim::address::Address,
) -> anyhow::Result<()> {
    use super::fvm_shared_latest::{
        address::Protocol::Delegated,
        crypto::signature::{ops::recover_secp_public_key, SECP_SIG_LEN},
    };
    use crate::rpc::eth::types::EthAddress;
    use crate::utils::encoding::keccak_256;

    anyhow::ensure!(
        addr.protocol() == Delegated,
        "cannot validate a delegated signature against a {} address expected",
        addr.protocol()
    );

    anyhow::ensure!(
        signature.len() == SECP_SIG_LEN,
        "invalid delegated signature length. Was {}, must be {}",
        signature.len(),
        SECP_SIG_LEN
    );

    let hash = keccak_256(data);
    let mut sig = [0u8; SECP_SIG_LEN];
    sig[..].copy_from_slice(signature);
    let pub_key = recover_secp_public_key(&hash, &sig)?;

    let eth_addr = EthAddress::eth_address_from_pub_key(&pub_key.serialize())?;

    let rec_addr = eth_addr.to_filecoin_address()?;

    // check address against recovered address
    anyhow::ensure!(rec_addr == *addr, "Delegated signature verification failed");

    Ok(())
}

/// Extracts the raw replica commitment from a CID
/// assuming that it has the correct hashing function and
/// serialization types
pub fn cid_to_replica_commitment_v1(c: &Cid) -> Result<Commitment, &'static str> {
    fvm_shared_latest::commcid::cid_to_replica_commitment_v1(c)
}

/// Signature variants for Filecoin signatures.
#[derive(
    Clone,
    Debug,
    PartialEq,
    FromPrimitive,
    Copy,
    Eq,
    Serialize_repr,
    Deserialize_repr,
    Hash,
    strum::Display,
    strum::EnumString,
    JsonSchema,
)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[repr(u8)]
#[strum(serialize_all = "lowercase")]
pub enum SignatureType {
    Secp256k1 = 1,
    Bls = 2,
    Delegated = 3,
}

impl TryFrom<u8> for SignatureType {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(SignatureType::Secp256k1),
            2 => Ok(SignatureType::Bls),
            3 => Ok(SignatureType::Delegated),
            invalid => anyhow::bail!("Invalid signature type byte: {}", invalid),
        }
    }
}

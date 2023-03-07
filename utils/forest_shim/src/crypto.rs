// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::ops::{Deref, DerefMut};

pub use fvm_shared::crypto::signature::{
    Signature as Signature_v2, SignatureType as SignatureType_v2,
};
pub use fvm_shared3::crypto::signature::{
    Signature as Signature_v3, SignatureType as SignatureType_v3,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, Hash)]
#[repr(transparent)]
#[serde(transparent)]
pub struct Signature(pub Signature_v3);

impl Signature {
    pub fn new(sig_type: SignatureType, bytes: Vec<u8>) -> Self {
        Signature(Signature_v3 {
            sig_type: *sig_type,
            bytes,
        })
    }

    /// Creates a BLS Signature given the raw bytes.
    pub fn new_bls(bytes: Vec<u8>) -> Self {
        Signature(Signature_v3::new_bls(bytes))
    }

    /// Creates a SECP Signature given the raw bytes.
    pub fn new_secp256k1(bytes: Vec<u8>) -> Self {
        Signature(Signature_v3::new_secp256k1(bytes))
    }

    pub fn signature_type(&self) -> SignatureType {
        self.0.signature_type().into()
    }
}

impl quickcheck::Arbitrary for Signature {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Signature(Signature_v3::arbitrary(g))
    }
}

impl Deref for Signature {
    type Target = Signature_v3;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Signature {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<Signature_v2> for Signature {
    fn from(value: Signature_v2) -> Self {
        let sig_type: SignatureType = value.signature_type().into();
        let sig: Signature_v3 = Signature_v3 {
            sig_type: sig_type.into(),
            bytes: value.bytes().into(),
        };
        Signature(sig)
    }
}

impl From<Signature_v3> for Signature {
    fn from(value: Signature_v3) -> Self {
        Signature(value)
    }
}

impl From<Signature> for Signature_v3 {
    fn from(other: Signature) -> Self {
        Signature_v3 {
            sig_type: other.signature_type().into(),
            bytes: other.bytes().into(),
        }
    }
}

impl From<&Signature> for Signature_v2 {
    fn from(other: &Signature) -> Signature_v2 {
        let sig_type: SignatureType = other.signature_type();
        let sig: Signature_v2 = Signature_v2 {
            sig_type: sig_type.into(),
            bytes: other.bytes().into(),
        };
        sig
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, Serialize, Deserialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct SignatureType(pub SignatureType_v3);

impl SignatureType {
    pub const BLS: Self = Self(SignatureType_v3::BLS);
    #[allow(non_upper_case_globals)]
    pub const Secp256k1: Self = Self(SignatureType_v3::Secp256k1);
    #[allow(non_upper_case_globals)]
    pub const Delegated: Self = Self(SignatureType_v3::Delegated);
}

impl quickcheck::Arbitrary for SignatureType {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        SignatureType(SignatureType_v3::arbitrary(g))
    }
}

impl Deref for SignatureType {
    type Target = SignatureType_v3;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SignatureType {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<SignatureType_v2> for SignatureType {
    fn from(value: SignatureType_v2) -> Self {
        match value {
            SignatureType_v2::Secp256k1 => SignatureType(SignatureType_v3::Secp256k1),
            SignatureType_v2::BLS => SignatureType(SignatureType_v3::BLS),
        }
    }
}

impl From<SignatureType_v3> for SignatureType {
    fn from(value: SignatureType_v3) -> Self {
        SignatureType(value)
    }
}

impl From<SignatureType> for SignatureType_v3 {
    fn from(other: SignatureType) -> Self {
        other.0
    }
}

impl From<SignatureType> for SignatureType_v2 {
    fn from(other: SignatureType) -> SignatureType_v2 {
        match other.0 {
            SignatureType_v3::Secp256k1 => SignatureType_v2::Secp256k1,
            SignatureType_v3::BLS => SignatureType_v2::BLS,
            SignatureType_v3::Delegated => panic!("Delegated signature type not possible in fvm2"),
        }
    }
}

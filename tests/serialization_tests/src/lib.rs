// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "serde_tests")]

use crypto::Signature;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SignatureVector {
    #[serde(alias = "Type")]
    sig_type: u8,
    #[serde(alias = "Data")]
    data: String,
}

impl From<SignatureVector> for Signature {
    fn from(v: SignatureVector) -> Self {
        match v.sig_type {
            1 => Signature::new_secp256k1(base64::decode(&v.data).unwrap()),
            2 => Signature::new_bls(base64::decode(&v.data).unwrap()),
            _ => panic!("unsupported signature type"),
        }
    }
}

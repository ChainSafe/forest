// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::signature::Signature;
use address::Address;
use std::error::Error;

/// Signer is a trait which allows a key implementation to sign data for an address
pub trait Signer {
    /// Function signs any arbitrary data given the [Address].
    fn sign_bytes(&self, data: &[u8], address: &Address) -> Result<Signature, Box<dyn Error>>;
}

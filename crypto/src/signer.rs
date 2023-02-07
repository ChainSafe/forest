// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::{address::Address, crypto::signature::Signature};

/// Signer is a trait which allows a key implementation to sign data for an
/// address
pub trait Signer {
    /// Function signs any arbitrary data given the [Address].
    fn sign_bytes(&self, data: &[u8], address: &Address) -> Result<Signature, anyhow::Error>;
}

// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

impl PublicKeyOnG2 {
    pub fn as_affine(&self) -> G2Affine {
        self.0.to_affine()
    }

    pub fn verify(&self, message: impl AsRef<[u8]>, signature: &SignatureOnG1) -> bool {
        verify_messages_unchained(self, &[message.as_ref()], &[signature])
    }

    pub fn verify_batch(&self, messages: &[&[u8]], signatures: &[&SignatureOnG1]) -> bool {
        verify_messages_unchained(self, messages, signatures)
    }

    pub fn from_bytes(raw: &[u8]) -> Result<Self, Error> {
        const SIZE: usize = G2Affine::compressed_size();

        if raw.len() != SIZE {
            return Err(Error::SizeMismatch);
        }

        let mut res = [0u8; SIZE];
        res.as_mut().copy_from_slice(raw);
        let affine: G2Affine =
            Option::from(G2Affine::from_compressed(&res)).ok_or(Error::GroupDecode)?;

        Ok(PublicKeyOnG2(affine.into()))
    }

    pub fn as_bytes(&self) -> [u8; G2Affine::compressed_size()] {
        self.0.to_affine().to_compressed()
    }
}

impl From<G2Projective> for PublicKeyOnG2 {
    fn from(val: G2Projective) -> Self {
        PublicKeyOnG2(val)
    }
}
impl From<PublicKeyOnG2> for G2Projective {
    fn from(val: PublicKeyOnG2) -> Self {
        val.0
    }
}

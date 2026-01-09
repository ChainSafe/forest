// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

impl From<G1Projective> for SignatureOnG1 {
    fn from(val: G1Projective) -> Self {
        SignatureOnG1(val.into())
    }
}
impl From<SignatureOnG1> for G1Projective {
    fn from(val: SignatureOnG1) -> Self {
        val.0.into()
    }
}

impl From<G1Affine> for SignatureOnG1 {
    fn from(val: G1Affine) -> Self {
        SignatureOnG1(val)
    }
}

impl From<SignatureOnG1> for G1Affine {
    fn from(val: SignatureOnG1) -> Self {
        val.0
    }
}

fn g1_from_slice(raw: &[u8]) -> Result<G1Affine, Error> {
    const SIZE: usize = G1Affine::compressed_size();

    if raw.len() != SIZE {
        return Err(Error::SizeMismatch);
    }

    let mut res = [0u8; SIZE];
    res.copy_from_slice(raw);

    Option::from(G1Affine::from_compressed(&res)).ok_or(Error::GroupDecode)
}

impl SignatureOnG1 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, Error> {
        let g1 = g1_from_slice(raw)?;
        Ok(g1.into())
    }

    pub fn as_bytes(&self) -> [u8; G1Affine::compressed_size()] {
        self.0.to_compressed()
    }
}

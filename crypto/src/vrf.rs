// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::signature::{verify_bls_sig, Signature, BLS_SIG_LEN};
use bls_signatures::{Serialize as BlsSerialize, Signature as BLSSignature};
use encoding::serde_bytes;
use serde::{Deserialize, Serialize};

pub struct VRFPublicKey(Vec<u8>);

/// Contains some public key type to be used for VRF verification
impl VRFPublicKey {
    pub fn new(input: Vec<u8>) -> Self {
        Self(input)
    }
}

/// The output from running a VRF
#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VRFResult(#[serde(with = "serde_bytes")] Vec<u8>);

impl VRFResult {
    /// Creates a VRFResult from a raw vector
    pub fn new(output: Vec<u8>) -> Self {
        Self(output)
    }
    /// Returns reference to underlying vector
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
    /// Returns max value based on [BLS_SIG_LEN](constant.BLS_SIG_LEN.html)
    pub fn max_value() -> Self {
        Self::new([std::u8::MAX; BLS_SIG_LEN].to_vec())
    }
    /// Validates syntax...
    pub fn validate_syntax(&self) -> bool {
        unimplemented!()
    }
    /// Asserts whether `input` was used with `pk` to produce this VRFOutput
    pub fn verify(&self, input: &[u8], pk: &VRFPublicKey) -> bool {
        match BLSSignature::from_bytes(&self.0) {
            Ok(sig) => verify_bls_sig(input, &pk.0, &Signature::new_bls(sig.as_bytes())),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bls_signatures::{PrivateKey, Serialize};
    use rand::rngs::mock::StepRng;
    use rand::Rng;

    #[test]
    fn verify() {
        let rng = &mut StepRng::new(4, 3);
        let privk = PrivateKey::generate(rng);

        let init_input = (0..64).map(|_| rng.gen()).collect::<Vec<u8>>();
        let input = privk.sign(&init_input);

        let genesis = VRFResult::new(input.as_bytes());

        let sig = privk.sign(genesis.bytes());
        let res = VRFResult::new(sig.as_bytes());

        let pubk = VRFPublicKey::new(privk.public_key().as_bytes());

        assert!(res.verify(genesis.bytes(), &pubk));
    }
}

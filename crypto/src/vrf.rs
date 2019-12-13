#![allow(dead_code)]

use crate::signature::verify_bls_sig;
use crate::signature::BLS_SIG_LEN;
use bls_signatures::{Serialize, Signature};

struct VRFPublicKey(Vec<u8>);

/// VRFResult is the output from running a VRF
#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd)]
pub struct VRFResult(Vec<u8>);

impl VRFResult {
    pub fn new(output: Vec<u8>) -> Self {
        Self(output)
    }
    pub fn as_bytes(&self) -> Vec<u8> {
        self.0.clone()
    }
    /// Returns max value based on `BLS_SIG_LEN`
    fn max_value() -> Vec<u8> {
        vec![std::u8::MAX; BLS_SIG_LEN]
    }
    fn validate_syntax() -> bool {
        unimplemented!()
    }
    /// Asserts whether `input` was used with `pk` to produce `Self.output`
    fn verify(&self, input: Vec<u8>, pk: VRFPublicKey) -> bool {
        match Signature::from_bytes(&self.0) {
            Ok(sig) => verify_bls_sig(input, pk.0, sig.as_bytes()),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::is_valid_signature;
    use bls_signatures::{PrivateKey, Serialize};
    use rand::{Rng, SeedableRng, XorShiftRng};

    #[test]
    fn verify() {
        let rng = &mut XorShiftRng::from_seed([0x3dbe6259, 0x8d313d76, 0x3237db17, 0xe5bc0654]);
        let privk = PrivateKey::generate(rng);

        let init_input = (0..64).map(|_| rng.gen()).collect::<Vec<u8>>();
        let input = privk.sign(&init_input);

        let genesis = VRFResult::new(input.as_bytes());

        let sig = privk.sign(&genesis.as_bytes());
        let res = VRFResult::new(sig.as_bytes());

        let pubk = VRFPublicKey(privk.public_key().as_bytes());

        assert!(res.verify(genesis.as_bytes(), pubk));
    }
}

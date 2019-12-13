use crate::signature::BLS_SIG_LEN;
use crate::signature::verify_bls_sig;
use bls_signatures::{Signature, Serialize};

struct VRFPublicKey(Vec<u8>);


struct VRFResult {
    output: Vec<u8>
}

impl VRFResult {
    fn new(output: Vec<u8>) -> Self {
        Self { output }
    }
    fn max_value() -> Vec<u8> {
        vec![std::u8::MAX; BLS_SIG_LEN]
    }
    fn validate_syntax() -> bool {
        unimplemented!()
    }
    fn verify(&self, input: Vec<u8>, pk: VRFPublicKey) -> bool {
        match Signature::from_bytes(&self.output) {
            Ok(sig) => {
                verify_bls_sig(input, pk.0, sig.as_bytes())
            },
            Err(_) => {
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bls_signatures::{PrivateKey, Serialize};
    use rand::{Rng, SeedableRng, XorShiftRng};
    use crate::is_valid_signature;

    #[test]
    fn verify() {
        let rng = &mut XorShiftRng::from_seed([0x3dbe6259, 0x8d313d76, 0x3237db17, 0xe5bc0654]);
        let privk = PrivateKey::generate(rng);

        let init_input = (0..64).map(|_| rng.gen()).collect::<Vec<u8>>();
        let input = privk.sign(&init_input);

        let genesis = VRFResult{ output: input.as_bytes() };

        let sig = privk.sign(&genesis.output);
        let res = VRFResult{ output: sig.as_bytes() };

        let pubk = VRFPublicKey(privk.public_key().as_bytes());

        assert!(res.verify(genesis.output.clone(), pubk));
    }
}
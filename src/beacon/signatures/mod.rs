// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod public_key_impls;
mod signature_impls;

use bls_signatures::Error;
use blstrs::{G1Affine, G1Projective, G2Affine, G2Projective};
use group::{prime::PrimeCurveAffine, Curve};
use rayon::prelude::*;

// re-exports
pub use bls_signatures::{PublicKey as PublicKeyOnG1, Signature as SignatureOnG2};

// See <https://www.ietf.org/archive/id/draft-irtf-cfrg-bls-signature-05.html#name-basic>
const CSUITE_G1: &[u8] = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_";
const CSUITE_G2: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_NUL_";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PublicKeyOnG2(pub(crate) G2Projective);

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SignatureOnG1(pub(crate) G1Affine);

/// Ported from <https://docs.rs/bls-signatures/0.15.0/src/bls_signatures/signature.rs.html#214>
pub fn verify_messages_unchained(
    public_key: &PublicKeyOnG2,
    messages: &[&[u8]],
    signatures: &[&SignatureOnG1],
) -> bool {
    let n_messages = messages.len();
    if n_messages != signatures.len() {
        return false;
    }
    if n_messages == 0 {
        return true;
    }

    let public_key: G2Affine = public_key.as_affine();
    // zero key & single message should fail
    if n_messages == 1 && public_key.is_identity().into() {
        return false;
    }

    // Enforce that messages are distinct as a countermeasure against BLS's rogue-key attack.
    // See Section 3.1. of the IRTF's BLS signatures spec:
    // https://tools.ietf.org/html/draft-irtf-cfrg-bls-signature-02#section-3.1
    if !blstrs::unique_messages(messages) {
        return false;
    }

    let n_workers = std::cmp::min(rayon::current_num_threads(), n_messages);
    let Some(Ok(acc)) = messages
        .par_iter()
        .zip(signatures.par_iter())
        .chunks(n_messages / n_workers)
        .map(|chunk| {
            let mut pairing = blstrs::PairingG2G1::new(true, CSUITE_G1);
            for (message, signature) in chunk {
                pairing
                    .aggregate(&public_key, Some(&signature.0), message, &[])
                    .map_err(map_blst_error)?;
            }
            pairing.commit();
            anyhow::Ok(pairing)
        })
        .try_reduce_with(|mut acc, pairing| {
            acc.merge(&pairing).map_err(map_blst_error)?;
            anyhow::Ok(acc)
        })
    else {
        return false;
    };

    acc.finalverify(None)
}

/// Ported from <https://docs.rs/bls-signatures/0.15.0/src/bls_signatures/signature.rs.html#214>
pub fn verify_messages_chained(
    public_key: &PublicKeyOnG1,
    messages: &[&[u8]],
    signatures: &[SignatureOnG2],
) -> bool {
    let n_messages = messages.len();
    if n_messages != signatures.len() {
        return false;
    }
    if n_messages == 0 {
        return true;
    }

    let public_key: G1Affine = public_key.as_affine();
    // zero key & single message should fail
    if n_messages == 1 && public_key.is_identity().into() {
        return false;
    }

    // Enforce that messages are distinct as a countermeasure against BLS's rogue-key attack.
    // See Section 3.1. of the IRTF's BLS signatures spec:
    // https://tools.ietf.org/html/draft-irtf-cfrg-bls-signature-02#section-3.1
    if !blstrs::unique_messages(messages) {
        return false;
    }

    let n_workers = std::cmp::min(rayon::current_num_threads(), n_messages);
    let Some(Ok(acc)) = messages
        .par_iter()
        .zip(signatures.par_iter())
        .chunks(n_messages / n_workers)
        .map(|chunk| {
            let mut pairing = blstrs::PairingG1G2::new(true, CSUITE_G2);
            for (message, signature) in chunk {
                pairing
                    .aggregate(&public_key, Some(&(*signature).into()), message, &[])
                    .map_err(map_blst_error)?;
            }
            pairing.commit();
            anyhow::Ok(pairing)
        })
        .try_reduce_with(|mut acc, pairing| {
            acc.merge(&pairing).map_err(map_blst_error)?;
            anyhow::Ok(acc)
        })
    else {
        return false;
    };

    acc.finalverify(None)
}

fn map_blst_error(e: impl std::fmt::Debug) -> anyhow::Error {
    anyhow::anyhow!("{e:?}")
}

#[cfg(test)]
mod tests;

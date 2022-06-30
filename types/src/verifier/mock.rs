// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::ProofVerifier;
use crate::{PoStProof, Randomness, RegisteredPoStProof, SealVerifyInfo, SectorInfo};

/// Mock verifier. This does no-op verification of any proofs.
pub enum MockVerifier {}

impl ProofVerifier for MockVerifier {
    fn verify_seal(_: &SealVerifyInfo) -> Result<(), anyhow::Error> {
        Ok(())
    }
    fn verify_winning_post(
        _: Randomness,
        _: &[PoStProof],
        _: &[SectorInfo],
        _: u64,
    ) -> Result<(), anyhow::Error> {
        Ok(())
    }
    fn verify_window_post(
        _: Randomness,
        _: &[PoStProof],
        _: &[SectorInfo],
        _: u64,
    ) -> Result<bool, anyhow::Error> {
        Ok(true)
    }
    fn generate_winning_post_sector_challenge(
        _: RegisteredPoStProof,
        _: u64,
        _: Randomness,
        _: u64,
    ) -> Result<Vec<u64>, anyhow::Error> {
        Ok(vec![0])
    }
}

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::ProofVerifier;
use crate::{PoStProof, SealVerifyInfo, SectorInfo};
use std::error::Error as StdError;

/// Verifier implementation
pub enum MockVerifier {}

impl ProofVerifier for MockVerifier {
    fn verify_seal(_: &SealVerifyInfo) -> Result<(), Box<dyn StdError>> {
        Ok(())
    }
    fn verify_winning_post(
        _: [u8; 32],
        _: &[PoStProof],
        _: &[SectorInfo],
        _: u64,
    ) -> Result<(), Box<dyn StdError>> {
        Ok(())
    }
    fn verify_window_post(
        _: [u8; 32],
        _: &[PoStProof],
        _: &[SectorInfo],
        _: u64,
    ) -> Result<(), Box<dyn StdError>> {
        Ok(())
    }
}

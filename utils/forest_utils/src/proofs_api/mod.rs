// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod paramfetch;

/// Mod that contains wrappers for functions in [`filecoin_proofs_api::post`]
/// that ensure parameter files are downloaded.
pub mod post {
    #![allow(clippy::disallowed_types)]

    use std::collections::BTreeMap;

    use filecoin_proofs_api::{
        post, ChallengeSeed, ProverId, PublicReplicaInfo, RegisteredPoStProof, SectorId,
    };

    use super::paramfetch;

    /// Wrapper of
    /// [`filecoin_proofs_api::post::generate_winning_post_sector_challenge`]
    /// that ensures parameter files are downloaded.
    pub async fn generate_winning_post_sector_challenge(
        proof_type: RegisteredPoStProof,
        randomness: &ChallengeSeed,
        sector_set_len: u64,
        prover_id: ProverId,
    ) -> anyhow::Result<Vec<u64>> {
        paramfetch::ensure_params_downloaded().await?;
        post::generate_winning_post_sector_challenge(
            proof_type,
            randomness,
            sector_set_len,
            prover_id,
        )
    }

    /// Wrapper of [`filecoin_proofs_api::post::verify_winning_post`]
    /// that ensures parameter files are downloaded.
    pub async fn verify_winning_post(
        randomness: &ChallengeSeed,
        proof: &[u8],
        replicas: &BTreeMap<SectorId, PublicReplicaInfo>,
        prover_id: ProverId,
    ) -> anyhow::Result<bool> {
        paramfetch::ensure_params_downloaded().await?;
        post::verify_winning_post(randomness, proof, replicas, prover_id)
    }
}

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{
    OnChainElectionPoStVerifyInfo, OnChainPoStVerifyInfo, OnChainSealVerifyInfo, PoStCandidate,
    PoStProof, PrivatePoStCandidateProof, SealVerifyInfo, SectorID,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

impl Serialize for SectorID {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.miner, &self.number).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SectorID {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (miner, number) = Deserialize::deserialize(deserializer)?;
        Ok(Self { miner, number })
    }
}

impl Serialize for SealVerifyInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.sector_id,
            &self.on_chain,
            &self.randomness,
            &self.interactive_randomness,
            &self.unsealed_cid,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SealVerifyInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (sector_id, on_chain, randomness, interactive_randomness, unsealed_cid) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            sector_id,
            on_chain,
            randomness,
            interactive_randomness,
            unsealed_cid,
        })
    }
}

impl Serialize for OnChainSealVerifyInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.sealed_cid,
            &self.interactive_epoch,
            &self.registered_proof,
            &self.proof,
            &self.deal_ids,
            &self.sector_num,
            &self.seal_rand_epoch,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OnChainSealVerifyInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (
            sealed_cid,
            interactive_epoch,
            registered_proof,
            proof,
            deal_ids,
            sector_num,
            seal_rand_epoch,
        ) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            sealed_cid,
            interactive_epoch,
            registered_proof,
            proof,
            deal_ids,
            sector_num,
            seal_rand_epoch,
        })
    }
}

impl Serialize for PoStCandidate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.registered_proof,
            &self.ticket,
            &self.private_proof,
            &self.sector_id,
            &self.challenge_index,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PoStCandidate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (registered_proof, ticket, private_proof, sector_id, challenge_index) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            registered_proof,
            ticket,
            private_proof,
            sector_id,
            challenge_index,
        })
    }
}

impl Serialize for PoStProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.registered_proof, &self.proof_bytes).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PoStProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (registered_proof, proof_bytes) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            registered_proof,
            proof_bytes,
        })
    }
}

impl Serialize for PrivatePoStCandidateProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.registered_proof, &self.externalized).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PrivatePoStCandidateProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (registered_proof, externalized) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            registered_proof,
            externalized,
        })
    }
}

impl Serialize for OnChainPoStVerifyInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.candidates, &self.proofs).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OnChainPoStVerifyInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (candidates, proofs) = Deserialize::deserialize(deserializer)?;
        Ok(Self { candidates, proofs })
    }
}

impl Serialize for OnChainElectionPoStVerifyInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.candidates, &self.proofs).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OnChainElectionPoStVerifyInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (candidates, proofs) = Deserialize::deserialize(deserializer)?;
        Ok(Self { candidates, proofs })
    }
}

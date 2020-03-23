// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{
    InteractiveSealRandomness, OnChainElectionPoStVerifyInfo, OnChainPoStVerifyInfo,
    OnChainSealVerifyInfo, PartialTicket, PoStCandidate, PoStProof, PrivatePoStCandidateProof,
    SealRandomness, SealVerifyInfo, SectorID,
};
use encoding::serde_bytes::{ByteBuf, Bytes};
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
            Bytes::new(&self.randomness),
            Bytes::new(&self.interactive_randomness),
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
        let (sector_id, on_chain, r_buf, ir_buf, unsealed_cid): (_, _, ByteBuf, ByteBuf, _) =
            Deserialize::deserialize(deserializer)?;

        let mut randomness: SealRandomness = Default::default();
        randomness.copy_from_slice(r_buf.as_ref());
        let mut interactive_randomness: InteractiveSealRandomness = Default::default();
        interactive_randomness.copy_from_slice(ir_buf.as_ref());

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
            Bytes::new(&self.proof),
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
        ): (_, _, _, ByteBuf, _, _, _) = Deserialize::deserialize(deserializer)?;

        Ok(Self {
            sealed_cid,
            interactive_epoch,
            registered_proof,
            proof: proof.into_vec(),
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
            Bytes::new(&self.ticket),
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
        let (registered_proof, t_buf, private_proof, sector_id, challenge_index): (
            _,
            ByteBuf,
            _,
            _,
            _,
        ) = Deserialize::deserialize(deserializer)?;

        let mut ticket: PartialTicket = Default::default();
        ticket.copy_from_slice(t_buf.as_ref());

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
        (&self.registered_proof, Bytes::new(&self.proof_bytes)).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PoStProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (registered_proof, proof_bytes): (_, ByteBuf) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            registered_proof,
            proof_bytes: proof_bytes.into_vec(),
        })
    }
}

impl Serialize for PrivatePoStCandidateProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.registered_proof, Bytes::new(&self.externalized)).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PrivatePoStCandidateProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (registered_proof, e_buf): (_, ByteBuf) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            registered_proof,
            externalized: e_buf.into_vec(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use encoding::{from_slice, to_vec};

    #[test]
    fn default_serializations() {
        let s = SealVerifyInfo::default();
        let bz = to_vec(&s).unwrap();
        assert_eq!(from_slice::<SealVerifyInfo>(&bz).unwrap(), s);

        let s = OnChainSealVerifyInfo::default();
        let bz = to_vec(&s).unwrap();
        assert_eq!(from_slice::<OnChainSealVerifyInfo>(&bz).unwrap(), s);

        let s = PoStCandidate::default();
        let bz = to_vec(&s).unwrap();
        assert_eq!(from_slice::<PoStCandidate>(&bz).unwrap(), s);

        let s = PoStProof::default();
        let bz = to_vec(&s).unwrap();
        assert_eq!(from_slice::<PoStProof>(&bz).unwrap(), s);

        let s = PoStProof::default();
        let bz = to_vec(&s).unwrap();
        assert_eq!(from_slice::<PoStProof>(&bz).unwrap(), s);

        let s = OnChainPoStVerifyInfo::default();
        let bz = to_vec(&s).unwrap();
        assert_eq!(from_slice::<OnChainPoStVerifyInfo>(&bz).unwrap(), s);

        let s = OnChainElectionPoStVerifyInfo::default();
        let bz = to_vec(&s).unwrap();
        assert_eq!(from_slice::<OnChainElectionPoStVerifyInfo>(&bz).unwrap(), s);
    }
}

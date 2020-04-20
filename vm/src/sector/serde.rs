// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{
    OnChainSealVerifyInfo, OnChainWindowPoStVerifyInfo, PoStProof, SealVerifyInfo, SectorID,
    SectorInfo, WindowPoStVerifyInfo, WinningPoStVerifyInfo,
};
use encoding::{Byte32De, BytesDe, BytesSer};
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
            BytesSer(&self.randomness),
            BytesSer(&self.interactive_randomness),
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
        let (
            sector_id,
            on_chain,
            Byte32De(randomness),
            Byte32De(interactive_randomness),
            unsealed_cid,
        ) = Deserialize::deserialize(deserializer)?;

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
            BytesSer(&self.proof),
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
            BytesDe(proof),
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

impl Serialize for SectorInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.proof, &self.sector_number, &self.sealed_cid).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SectorInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (proof, sector_number, sealed_cid) = Deserialize::deserialize(deserializer)?;

        Ok(Self {
            proof,
            sector_number,
            sealed_cid,
        })
    }
}

impl Serialize for WindowPoStVerifyInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            BytesSer(&self.randomness),
            &self.proofs,
            &self.private_proof,
            &self.prover,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WindowPoStVerifyInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (Byte32De(randomness), proofs, private_proof, prover) =
            Deserialize::deserialize(deserializer)?;

        Ok(Self {
            randomness,
            proofs,
            private_proof,
            prover,
        })
    }
}

impl Serialize for WinningPoStVerifyInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            BytesSer(&self.randomness),
            &self.proofs,
            &self.challenge_sectors,
            &self.prover,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WinningPoStVerifyInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (Byte32De(randomness), proofs, challenge_sectors, prover) =
            Deserialize::deserialize(deserializer)?;

        Ok(Self {
            randomness,
            proofs,
            challenge_sectors,
            prover,
        })
    }
}

impl Serialize for PoStProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.registered_proof, BytesSer(&self.proof_bytes)).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PoStProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (registered_proof, BytesDe(proof_bytes)) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            registered_proof,
            proof_bytes,
        })
    }
}

impl Serialize for OnChainWindowPoStVerifyInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [&self.proofs].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OnChainWindowPoStVerifyInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [proofs]: [Vec<PoStProof>; 1] = Deserialize::deserialize(deserializer)?;
        Ok(Self { proofs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cid::{multihash::Identity, Cid};
    use encoding::{from_slice, to_vec};

    fn empty_cid() -> Cid {
        Cid::new_from_cbor(&[], Identity)
    }

    #[test]
    fn default_serializations() {
        let ocs = OnChainSealVerifyInfo {
            sealed_cid: empty_cid(),
            ..Default::default()
        };
        let bz = to_vec(&ocs).unwrap();
        assert_eq!(from_slice::<OnChainSealVerifyInfo>(&bz).unwrap(), ocs);

        let s = SealVerifyInfo {
            unsealed_cid: empty_cid(),
            on_chain: ocs,
            ..Default::default()
        };
        let bz = to_vec(&s).unwrap();
        assert_eq!(from_slice::<SealVerifyInfo>(&bz).unwrap(), s);

        let s = WindowPoStVerifyInfo::default();
        let bz = to_vec(&s).unwrap();
        assert_eq!(from_slice::<WindowPoStVerifyInfo>(&bz).unwrap(), s);

        let s = PoStProof::default();
        let bz = to_vec(&s).unwrap();
        assert_eq!(from_slice::<PoStProof>(&bz).unwrap(), s);

        let s = PoStProof::default();
        let bz = to_vec(&s).unwrap();
        assert_eq!(from_slice::<PoStProof>(&bz).unwrap(), s);

        let s = OnChainWindowPoStVerifyInfo::default();
        let bz = to_vec(&s).unwrap();
        assert_eq!(from_slice::<OnChainWindowPoStVerifyInfo>(&bz).unwrap(), s);
    }
}

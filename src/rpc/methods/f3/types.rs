// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::{
    blocks::{Tipset, TipsetKey},
    lotus_json::{base64_standard, lotus_json_with_self, HasLotusJson},
    networks::NetworkChain,
};
use cid::{multihash::MultihashDigest as _, Cid};
use fvm_ipld_encoding::tuple::{Deserialize_tuple, Serialize_tuple};
use fvm_shared4::ActorID;
use itertools::Itertools as _;
use libp2p::PeerId;
use once_cell::sync::Lazy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use std::cmp::Ordering;

/// TipSetKey is the canonically ordered concatenation of the block CIDs in a tipset.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct F3TipSetKey(
    #[schemars(with = "String")]
    #[serde(with = "base64_standard")]
    pub Vec<u8>,
);
lotus_json_with_self!(F3TipSetKey);

impl From<&TipsetKey> for F3TipSetKey {
    fn from(tsk: &TipsetKey) -> Self {
        let bytes = tsk.iter().flat_map(|cid| cid.to_bytes()).collect();
        Self(bytes)
    }
}

impl From<TipsetKey> for F3TipSetKey {
    fn from(tsk: TipsetKey) -> Self {
        (&tsk).into()
    }
}

impl TryFrom<F3TipSetKey> for TipsetKey {
    type Error = anyhow::Error;

    fn try_from(tsk: F3TipSetKey) -> Result<Self, Self::Error> {
        static BLOCK_HEADER_CID_LEN: Lazy<usize> = Lazy::new(|| {
            let buf = [0_u8; 256];
            let cid = Cid::new_v1(
                fvm_ipld_encoding::DAG_CBOR,
                cid::multihash::Code::Blake2b256.digest(&buf),
            );
            cid.to_bytes().len()
        });

        let cids: Vec<Cid> = tsk
            .0
            .chunks(*BLOCK_HEADER_CID_LEN)
            .map(Cid::read_bytes)
            .try_collect()?;

        Ok(nunny::Vec::new(cids)
            .map_err(|_| anyhow::anyhow!("tipset key cannot be empty"))?
            .into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct F3TipSet {
    pub key: F3TipSetKey,
    /// The verifiable oracle randomness used to elect this block's author leader
    #[schemars(with = "String")]
    #[serde(with = "base64_standard")]
    pub beacon: Vec<u8>,
    /// The period in which a new block is generated.
    /// There may be multiple rounds in an epoch.
    pub epoch: ChainEpoch,
    /// Block creation time, in seconds since the Unix epoch
    pub timestamp: u64,
}
lotus_json_with_self!(F3TipSet);

impl From<Tipset> for F3TipSet {
    fn from(ts: Tipset) -> Self {
        let key = ts.key().into();
        let beacon = {
            let entries = &ts.block_headers().first().beacon_entries;
            if let Some(last) = entries.last() {
                last.signature().to_vec()
            } else {
                vec![0; 32]
            }
        };
        let epoch = ts.epoch();
        let timestamp = ts.block_headers().first().timestamp;
        Self {
            key,
            beacon,
            epoch,
            timestamp,
        }
    }
}

impl From<Arc<Tipset>> for F3TipSet {
    fn from(ts: Arc<Tipset>) -> Self {
        Arc::unwrap_or_clone(ts).into()
    }
}

/// PowerEntry represents a single entry in the PowerTable, including ActorID and its StoragePower and PubKey.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Eq, PartialEq)]
pub struct F3PowerEntry {
    #[serde(rename = "ID")]
    pub id: ActorID,
    #[schemars(with = "String")]
    #[serde(rename = "Power", with = "crate::lotus_json::stringify")]
    pub power: num::BigInt,
    #[schemars(with = "String")]
    #[serde(rename = "PubKey", with = "base64_standard")]
    pub pub_key: Vec<u8>,
}
lotus_json_with_self!(F3PowerEntry);

/// Entries are sorted descending order of their power, where entries with equal power are
/// sorted by ascending order of their ID.
/// This ordering is guaranteed to be stable, since a valid PowerTable cannot contain entries with duplicate IDs
impl Ord for F3PowerEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        match other.power.cmp(&self.power) {
            Ordering::Equal => self.id.cmp(&other.id),
            ord => ord,
        }
    }
}

impl PartialOrd for F3PowerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// represents a particular moment in the progress of GPBFT, captured by
/// instance ID, round and phase.
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct F3Instant {
    #[serde(rename = "ID")]
    id: u64,
    round: u64,
    phase: u8,
}
lotus_json_with_self!(F3Instant);

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct F3Participant {
    #[serde(rename = "MinerID")]
    pub miner_id: u64,
    pub from_instance: u64,
    pub validity_term: u64,
}
lotus_json_with_self!(F3Participant);

impl From<F3ParticipationLease> for F3Participant {
    fn from(value: F3ParticipationLease) -> Self {
        let F3ParticipationLease {
            miner_id,
            from_instance,
            validity_term,
            ..
        } = value;
        Self {
            miner_id,
            from_instance,
            validity_term,
        }
    }
}

impl From<&F3ParticipationLease> for F3Participant {
    fn from(value: &F3ParticipationLease) -> Self {
        let &F3ParticipationLease {
            miner_id,
            from_instance,
            validity_term,
            ..
        } = value;
        Self {
            miner_id,
            from_instance,
            validity_term,
        }
    }
}

/// defines the lease granted to a storage provider for
/// participating in F3 consensus, detailing the session identifier, issuer,
/// subject, and the expiration instance.
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ApiF3ParticipationLease {
    /// the name of the network this lease belongs to.
    #[schemars(with = "String")]
    pub network: NetworkChain,
    /// the identity of the node that issued the lease.
    #[schemars(with = "String")]
    pub issuer: PeerId,
    /// the actor ID of the miner that holds the lease.
    #[serde(rename = "MinerID")]
    pub miner_id: u64,
    /// specifies the instance ID from which this lease is valid.
    pub from_instance: u64,
    /// specifies the number of instances for which the lease remains valid from the FromInstance.
    pub validity_term: u64,
}

#[serde_as]
#[derive(PartialEq, Debug, Clone, Serialize_tuple, Deserialize_tuple)]
#[serde(rename_all = "PascalCase")]
pub struct F3ParticipationLease {
    #[serde_as(as = "DisplayFromStr")]
    pub network: NetworkChain,
    #[serde_as(as = "DisplayFromStr")]
    pub issuer: PeerId,
    pub miner_id: u64,
    pub from_instance: u64,
    pub validity_term: u64,
}

impl HasLotusJson for F3ParticipationLease {
    type LotusJson = ApiF3ParticipationLease;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let Self {
            network,
            issuer,
            miner_id,
            from_instance,
            validity_term,
        } = self;
        Self::LotusJson {
            network,
            issuer,
            miner_id,
            from_instance,
            validity_term,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            network,
            issuer,
            miner_id,
            from_instance,
            validity_term,
        } = lotus_json;
        Self {
            network,
            issuer,
            miner_id,
            from_instance,
            validity_term,
        }
    }
}

#[derive(Debug)]
pub struct F3LeaseManager {
    network: NetworkChain,
    peer_id: PeerId,
    leases: RwLock<HashMap<u64, F3ParticipationLease>>,
}

impl F3LeaseManager {
    pub fn new(network: NetworkChain, peer_id: PeerId) -> Self {
        Self {
            network,
            peer_id,
            leases: Default::default(),
        }
    }

    pub async fn get_active_participants(
        &self,
    ) -> anyhow::Result<HashMap<u64, F3ParticipationLease>> {
        let current_instance = super::F3GetProgress::run().await?.id;
        Ok(self
            .leases
            .read()
            .iter()
            .filter_map(|(id, lease)| {
                if lease.from_instance + lease.validity_term < current_instance {
                    Some((*id, lease.clone()))
                } else {
                    None
                }
            })
            .collect())
    }

    pub async fn get_or_renew_participation_lease(
        &self,
        id: u64,
        previous_lease: Option<F3ParticipationLease>,
        instances: u64,
    ) -> anyhow::Result<F3ParticipationLease> {
        const MAX_LEASE_INSTANCES: u64 = 5;

        anyhow::ensure!(instances > 0, "instances should be positive");
        anyhow::ensure!(
            instances <= MAX_LEASE_INSTANCES,
            "instances {instances} exceeds the maximum allowed value {MAX_LEASE_INSTANCES}"
        );

        let current_instance = super::F3GetProgress::run().await?.id;
        if let Some(previous_lease) = previous_lease {
            // A previous ticket is present. To avoid overlapping lease across multiple
            // instances for the same participant check its validity and only proceed to
            // issue a new ticket if: it is issued by this node for the same network.
            anyhow::ensure!(
                self.network == previous_lease.network,
                "the previous lease was issued for a different network"
            );
            anyhow::ensure!(
                self.peer_id == previous_lease.issuer,
                "the previous lease was not issued by this node"
            );
        }

        Ok(self.new_participation_lease(id, current_instance, instances))
    }

    fn new_participation_lease(
        &self,
        participant: u64,
        from_instance: u64,
        instances: u64,
    ) -> F3ParticipationLease {
        F3ParticipationLease {
            issuer: self.peer_id,
            network: self.network.clone(),
            miner_id: participant,
            from_instance,
            validity_term: instances,
        }
    }

    pub fn participate(&self, lease: F3ParticipationLease) -> anyhow::Result<F3ParticipationLease> {
        Ok(lease)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::prelude::*;

    #[test]
    fn decode_f3_participation_lease_ticket_from_lotus() {
        // ticket is generated by a Lotus ndoe by calling `Filecoin.F3GetOrRenewParticipationTicket`
        // params: ["t01000", "", 1]
        let ticket = "hW5jYWxpYnJhdGlvbm5ldHg0MTJEM0tvb1dKV0VxZzRLcXpxQUJMeU0yMUtBbWFKYzNqdFBzWEJrNmJNNllyN1BLSGczSxkD6AAB";
        let ticket_bytes = BASE64_STANDARD.decode(ticket).unwrap();
        let lease: F3ParticipationLease =
            fvm_ipld_encoding::from_slice(ticket_bytes.as_slice()).unwrap();
        assert_eq!(
            lease,
            F3ParticipationLease {
                network: NetworkChain::Calibnet,
                issuer: PeerId::from_str("12D3KooWJWEqg4KqzqABLyM21KAmaJc3jtPsXBk6bM6Yr7PKHg3K")
                    .unwrap(),
                miner_id: 1000,
                from_instance: 0,
                validity_term: 1,
            }
        );
    }

    #[test]
    fn f3_participation_lease_ticket_serde_roundtrip() {
        let lease = F3ParticipationLease {
            network: NetworkChain::Calibnet,
            issuer: PeerId::from_str("12D3KooWJWEqg4KqzqABLyM21KAmaJc3jtPsXBk6bM6Yr7PKHg3K")
                .unwrap(),
            miner_id: 1000,
            from_instance: 0,
            validity_term: 1,
        };
        let ticket = fvm_ipld_encoding::to_vec(&lease).unwrap();
        let decoded: F3ParticipationLease = fvm_ipld_encoding::from_slice(&ticket).unwrap();
        assert_eq!(lease, decoded);
    }
}

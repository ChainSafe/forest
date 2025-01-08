// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::utils::multihash::prelude::*;
use crate::{
    blocks::{Tipset, TipsetKey},
    lotus_json::{base64_standard, lotus_json_with_self, HasLotusJson, LotusJson},
    networks::NetworkChain,
};
use cid::Cid;
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fvm_ipld_encoding::tuple::{Deserialize_tuple, Serialize_tuple};
use fvm_shared4::ActorID;
use itertools::Itertools as _;
use libp2p::PeerId;
use num::Zero as _;
use once_cell::sync::Lazy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use std::{cmp::Ordering, time::Duration};

const MAX_LEASE_INSTANCES: u64 = 5;

/// TipSetKey is the canonically ordered concatenation of the block CIDs in a tipset.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
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
                MultihashCode::Blake2b256.digest(&buf),
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ECTipSet {
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json")]
    pub key: TipsetKey,
    pub epoch: ChainEpoch,
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json")]
    pub power_table: Cid,
    #[schemars(with = "String")]
    #[serde(with = "base64_standard")]
    pub commitments: Vec<u8>,
}
lotus_json_with_self!(ECTipSet);

/// PowerEntry represents a single entry in the PowerTable, including ActorID and its StoragePower and PubKey.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct F3PowerEntry {
    #[serde(rename = "ID")]
    pub id: ActorID,
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::stringify")]
    pub power: num::BigInt,
    #[schemars(with = "String")]
    #[serde(with = "base64_standard")]
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
    pub id: u64,
    pub round: u64,
    pub phase: u8,
}
lotus_json_with_self!(F3Instant);

impl F3Instant {
    pub fn phase_string(&self) -> &'static str {
        match self.phase {
            0 => "INITIAL",
            1 => "QUALITY",
            2 => "CONVERGE",
            3 => "PREPARE",
            4 => "COMMIT",
            5 => "DECIDE",
            6 => "TERMINATED",
            _ => "UNKNOWN",
        }
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct GpbftConfig {
    #[schemars(with = "u64")]
    #[serde(with = "crate::lotus_json")]
    pub delta: Duration,
    pub delta_back_off_exponent: f64,
    pub max_lookahead_rounds: u64,
    #[schemars(with = "u64")]
    #[serde(with = "crate::lotus_json")]
    pub rebroadcast_backoff_base: Duration,
    pub rebroadcast_backoff_exponent: f64,
    pub rebroadcast_backoff_spread: f64,
    #[schemars(with = "u64")]
    #[serde(with = "crate::lotus_json")]
    pub rebroadcast_backoff_max: Duration,
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct EcConfig {
    #[schemars(with = "u64")]
    #[serde(with = "crate::lotus_json")]
    pub period: Duration,
    pub finality: i64,
    pub delay_multiplier: f64,
    pub base_decision_backoff_table: Vec<f64>,
    pub head_lookback: i64,
    pub finalize: bool,
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct CertificateExchangeConfig {
    #[schemars(with = "u64")]
    #[serde(with = "crate::lotus_json")]
    pub client_request_timeout: Duration,
    #[schemars(with = "u64")]
    #[serde(with = "crate::lotus_json")]
    pub server_request_timeout: Duration,
    #[schemars(with = "u64")]
    #[serde(with = "crate::lotus_json")]
    pub minimum_poll_interval: Duration,
    #[schemars(with = "u64")]
    #[serde(with = "crate::lotus_json")]
    pub maximum_poll_interval: Duration,
}

#[serde_as]
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct F3Manifest {
    pub protocol_version: u64,
    pub pause: bool,
    pub initial_instance: u64,
    pub bootstrap_epoch: i64,
    pub network_name: String, // Note: NetworkChain::Calibnet.to_string() != "calibrationnet"
    #[schemars(with = "LotusJson<Vec<F3PowerEntry>>")]
    #[serde(with = "crate::lotus_json")]
    pub explicit_power: Vec<F3PowerEntry>,
    #[serde(rename = "IgnoreECPower")]
    pub ignore_ec_power: bool,
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json")]
    pub initial_power_table: Cid,
    pub committee_lookback: u64,
    #[schemars(with = "u64")]
    #[serde(with = "crate::lotus_json")]
    pub catch_up_alignment: Duration,
    pub gpbft: GpbftConfig,
    #[serde(rename = "EC")]
    pub ec: EcConfig,
    pub certificate_exchange: CertificateExchangeConfig,
}
lotus_json_with_self!(F3Manifest);

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct SupplementalData {
    #[schemars(with = "String")]
    #[serde(with = "base64_standard")]
    pub commitments: Vec<u8>,
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json")]
    pub power_table: Cid,
}
lotus_json_with_self!(SupplementalData);

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct PowerTableDelta {
    #[serde(rename = "ParticipantID")]
    pub participant_id: ActorID,
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::stringify")]
    pub power_delta: num::BigInt,
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json")]
    pub signing_key: Vec<u8>,
}
lotus_json_with_self!(PowerTableDelta);

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct FinalityCertificate {
    #[serde(rename = "GPBFTInstance")]
    pub instance: u64,
    #[schemars(with = "LotusJson<Vec<ECTipSet>>")]
    #[serde(rename = "ECChain", with = "crate::lotus_json")]
    pub ec_chain: Vec<ECTipSet>,
    #[schemars(with = "LotusJson<SupplementalData>")]
    #[serde(with = "crate::lotus_json")]
    pub supplemental_data: SupplementalData,
    #[schemars(with = "Vec<u8>")]
    #[serde(with = "crate::lotus_json")]
    pub signers: BitField,
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json")]
    pub signature: Vec<u8>,
    #[schemars(with = "LotusJson<Vec<PowerTableDelta>>")]
    #[serde(
        with = "crate::lotus_json",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub power_table_delta: Vec<PowerTableDelta>,
}
lotus_json_with_self!(FinalityCertificate);

impl FinalityCertificate {
    pub fn power_table_delta_string(&self) -> String {
        let total_diff = self
            .power_table_delta
            .iter()
            .map(|i| i.power_delta.clone())
            .fold(num::BigInt::zero(), |acc, x| acc + x);
        if total_diff.is_zero() {
            "None".into()
        } else {
            format!(
                "Total of {total_diff} storage power across {} miner(s).",
                self.power_table_delta.len()
            )
        }
    }

    pub fn chain_base(&self) -> &ECTipSet {
        // Switch to NonEmpty and drop `.expect`
        self.ec_chain.first().expect("ec_chain is empty")
    }

    pub fn chain_head(&self) -> &ECTipSet {
        // Switch to NonEmpty and drop `.expect`
        self.ec_chain.last().expect("ec_chain is empty")
    }
}

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

impl F3ParticipationLease {
    pub fn validate(
        &self,
        network: &NetworkChain,
        issuer: &PeerId,
        current_instance: u64,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            &self.network == network,
            "the ticket was not issued for the current network"
        );
        anyhow::ensure!(
            &self.issuer == issuer,
            "the ticket was not issued by the current node"
        );
        anyhow::ensure!(
            current_instance <= self.from_instance + self.validity_term,
            "the ticket has been expired"
        );
        anyhow::ensure!(
            self.validity_term <= MAX_LEASE_INSTANCES,
            "validity_term is too large"
        );
        Ok(())
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

    pub fn get_active_participants(
        &self,
        current_instance: u64,
    ) -> HashMap<u64, F3ParticipationLease> {
        self.leases
            .read()
            .iter()
            .filter_map(|(id, lease)| {
                if lease.from_instance + lease.validity_term >= current_instance {
                    Some((*id, lease.clone()))
                } else {
                    None
                }
            })
            .collect()
    }

    pub async fn get_or_renew_participation_lease(
        &self,
        id: u64,
        previous_lease: Option<F3ParticipationLease>,
        instances: u64,
    ) -> anyhow::Result<F3ParticipationLease> {
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

    pub fn participate(
        &self,
        lease: &F3ParticipationLease,
        current_instance: u64,
    ) -> anyhow::Result<()> {
        lease.validate(&self.network, &self.peer_id, current_instance)?;
        if let Some(old_lease) = self.leases.read().get(&lease.miner_id) {
            // This should never happen, adding this check just for logic completeness.
            anyhow::ensure!(
                old_lease.network == lease.network && old_lease.issuer == lease.issuer,
                "network or issuer mismatch"
            );
            // For safety, strictly require lease start instance to never decrease.
            anyhow::ensure!(
                lease.from_instance >= old_lease.from_instance,
                "the from instance should never decrease"
            );
        } else {
            tracing::info!("started participating in F3 for miner {}", lease.miner_id);
        }
        self.leases.write().insert(lease.miner_id, lease.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::prelude::*;

    #[test]
    fn decode_f3_participation_lease_ticket_from_lotus() {
        // ticket is generated from a Lotus node by calling `Filecoin.F3GetOrRenewParticipationTicket`
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

    #[test]
    fn f3_lease_manager_tests() {
        let network = NetworkChain::Calibnet;
        let peer_id = PeerId::random();
        let miner = 1000;

        let lm = F3LeaseManager::new(network, peer_id);

        let lease = lm.new_participation_lease(miner, 10, 2);
        assert!(
            lm.participate(&lease, 13).is_err(),
            "lease should be invalid when the current instance is 13"
        );

        // participate
        lm.participate(&lease, 11).unwrap();

        assert!(
            lm.participate(&lm.new_participation_lease(miner, 9, 2), 12)
                .is_err(),
            "from instance should never decrease"
        );

        // renew
        lm.participate(&lm.new_participation_lease(miner, 12, 4), 12)
            .unwrap();

        // The lease should be active at instance 13
        let active_participants = lm.get_active_participants(13);
        assert!(active_participants.contains_key(&miner));

        // The lease should be expired at instance 17
        let active_participants = lm.get_active_participants(17);
        assert!(!active_participants.contains_key(&miner));
    }

    #[test]
    fn f3_manifest_serde_roundtrip() {
        // lotus f3 manifest --output json
        let lotus_json = serde_json::json!({
          "Pause": false,
          "ProtocolVersion": 4,
          "InitialInstance": 0,
          "BootstrapEpoch": 2081674,
          "NetworkName": "calibrationnet",
          "ExplicitPower": null,
          "IgnoreECPower": false,
          "InitialPowerTable": {
            "/": "bafy2bzaceab236vmmb3n4q4tkvua2n4dphcbzzxerxuey3mot4g3cov5j3r2c"
          },
          "CommitteeLookback": 10,
          "CatchUpAlignment": 15000000000_u64,
          "Gpbft": {
            "Delta": 6000000000_u64,
            "DeltaBackOffExponent": 2_f64,
            "MaxLookaheadRounds": 5,
            "RebroadcastBackoffBase": 6000000000_u64,
            "RebroadcastBackoffExponent": 1.3,
            "RebroadcastBackoffSpread": 0.1,
            "RebroadcastBackoffMax": 60000000000_u64
          },
          "EC": {
            "Period": 30000000000_u64,
            "Finality": 900,
            "DelayMultiplier": 2_f64,
            "BaseDecisionBackoffTable": [
              1.3,
              1.69,
              2.2,
              2.86,
              3.71,
              4.83,
              6.27,
              7.5
            ],
            "HeadLookback": 0,
            "Finalize": true
          },
          "CertificateExchange": {
            "ClientRequestTimeout": 10000000000_u64,
            "ServerRequestTimeout": 60000000000_u64,
            "MinimumPollInterval": 30000000000_u64,
            "MaximumPollInterval": 120000000000_u64
          }
        });
        let manifest: F3Manifest = serde_json::from_value(lotus_json.clone()).unwrap();
        let serialized = serde_json::to_value(manifest.clone()).unwrap();
        assert_eq!(lotus_json, serialized);
    }

    #[test]
    fn f3_certificate_serde_roundtrip() {
        // lotus f3 c get --output json 6204
        let lotus_json = serde_json::json!({
          "GPBFTInstance": 6204,
          "ECChain": [
            {
              "Key": [
                {
                  "/": "bafy2bzacedknayz2ofrjwbjopek5aqz3z5whmtxk6xn35i2a2ydsrgsvnovzg"
                },
                {
                  "/": "bafy2bzacecndvxxvr7hgjdr2w5ezc5bvbk2n5vvocfw6fqwhbcxyimgtbhnpu"
                }
              ],
              "Commitments": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
              "Epoch": 2088927,
              "PowerTable": {
                "/": "bafy2bzaceazjn2promafvtkaquebfgc3xvhoavdbxwns4i54ilgnzch7pkgua"
              }
            },
            {
              "Key": [
                {
                  "/": "bafy2bzacealh6yg6v7ae5oawrfwniyms5o2n7tz2xegvqu7gkeugh7ga5jtze"
                },
                {
                  "/": "bafy2bzaceabmfeiw4d55ichcfrsrngeel2lprpk3qbtxmtkbjm5eaezxdpxbu"
                },
                {
                  "/": "bafy2bzacec4uupurmazrlwavzk3b5slsy4ye35mwpkepvt2ici3lwbhywvac6"
                },
                {
                  "/": "bafy2bzacedaybwo3l3dvdhvhdj43u7ttlxtfqxvhmc2nuzeysjemspp6ne5cq"
                }
              ],
              "Commitments": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
              "Epoch": 2088928,
              "PowerTable": {
                "/": "bafy2bzaceazjn2promafvtkaquebfgc3xvhoavdbxwns4i54ilgnzch7pkgua"
              }
            }
          ],
          "SupplementalData": {
            "Commitments": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "PowerTable": {
              "/": "bafy2bzaceazjn2promafvtkaquebfgc3xvhoavdbxwns4i54ilgnzch7pkgua"
            }
          },
          "Signers": [
            0,
            3
          ],
          "Signature": "uYtvw/NWm2jKQj+d99UAG4aiPnpAMSrwAWIusv0XkjsOYYR0fyU4nUM++cAQGO47E2/J8WSDjstLgL+yMVAFC+Tgao4o9ILXIlhqhxObnNZ/Ehanajthif9SaRe1AO69",
          "PowerTableDelta": [
            {
              "ParticipantID": 3782,
              "PowerDelta": "76347338653696",
              "SigningKey": "lXSMTNEVmIdVxJV4clmW35jrlsBEfytNUGTWVih2dFlQ1k/7QQttsUGzpD5JoNaQ"
            }
          ]
        });
        let cert: FinalityCertificate = serde_json::from_value(lotus_json.clone()).unwrap();
        let serialized = serde_json::to_value(cert.clone()).unwrap();
        assert_eq!(lotus_json, serialized);
    }
}

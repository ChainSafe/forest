// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actors_runtime::runtime::Policy;
use forest_beacon::{BeaconPoint, BeaconSchedule, DrandBeacon, DrandConfig};
use fvm_shared::clock::{ChainEpoch, EPOCH_DURATION_SECONDS};
use fvm_shared::sector::{RegisteredPoStProof, RegisteredSealProof, StoragePower};
use fvm_shared::version::NetworkVersion;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

pub mod calibnet;
mod drand;
pub mod mainnet;

/// Newest network version for all networks
pub const NEWEST_NETWORK_VERSION: NetworkVersion = NetworkVersion::V16;

/// Defines the meaningful heights of the protocol.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Height {
    Breeze,
    Smoke,
    Ignition,
    ActorsV2,
    Tape,
    Liftoff,
    Kumquat,
    Calico,
    Persian,
    Orange,
    Trust,
    Norwegian,
    Turbo,
    Hyperdrive,
    Chocolate,
    OhSnap,
    Skyr,
    Shark,
}

impl Default for Height {
    fn default() -> Height {
        Self::Breeze
    }
}

impl From<Height> for NetworkVersion {
    fn from(height: Height) -> NetworkVersion {
        match height {
            Height::Breeze => NetworkVersion::V1,
            Height::Smoke => NetworkVersion::V2,
            Height::Ignition => NetworkVersion::V3,
            Height::ActorsV2 => NetworkVersion::V4,
            Height::Tape => NetworkVersion::V5,
            Height::Liftoff => NetworkVersion::V5,
            Height::Kumquat => NetworkVersion::V6,
            Height::Calico => NetworkVersion::V7,
            Height::Persian => NetworkVersion::V8,
            Height::Orange => NetworkVersion::V9,
            Height::Trust => NetworkVersion::V10,
            Height::Norwegian => NetworkVersion::V11,
            Height::Turbo => NetworkVersion::V12,
            Height::Hyperdrive => NetworkVersion::V13,
            Height::Chocolate => NetworkVersion::V14,
            Height::OhSnap => NetworkVersion::V15,
            Height::Skyr => NetworkVersion::V16,
            Height::Shark => NetworkVersion::V17,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct UpgradeInfo {
    pub height: Height,
    #[serde(default = "default_network_version")]
    #[serde(with = "de_network_version")]
    pub version: NetworkVersion,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct HeightInfo {
    pub height: Height,
    pub epoch: ChainEpoch,
}

pub fn sort_by_epoch(height_info_slice: &[HeightInfo]) -> Vec<HeightInfo> {
    let mut height_info_vec = height_info_slice.to_vec();
    height_info_vec.sort_by(|a, b| a.epoch.cmp(&b.epoch));
    height_info_vec
}

#[derive(Clone)]
struct DrandPoint<'a> {
    pub height: ChainEpoch,
    pub config: &'a DrandConfig<'a>,
}

/// Defines all network configuration parameters.
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct ChainConfig {
    pub name: String,
    pub genesis_cid: Option<String>,
    pub bootstrap_peers: Vec<String>,
    pub block_delay_secs: u64,
    pub height_infos: Vec<HeightInfo>,
    #[serde(default = "default_policy")]
    #[serde(with = "serde_policy")]
    pub policy: Policy,
}

// FIXME: remove this trait once builtin-actors Policy have it
// https://github.com/filecoin-project/builtin-actors/pull/497
impl PartialEq for ChainConfig {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.bootstrap_peers == other.bootstrap_peers
            && self.block_delay_secs == other.block_delay_secs
            && sort_by_epoch(&self.height_infos) == sort_by_epoch(&other.height_infos)
            && (self.policy.max_aggregated_sectors == other.policy.max_aggregated_sectors
                && self.policy.min_aggregated_sectors == other.policy.min_aggregated_sectors
                && self.policy.max_aggregated_proof_size == other.policy.max_aggregated_proof_size
                && self.policy.max_replica_update_proof_size
                    == other.policy.max_replica_update_proof_size
                && self.policy.pre_commit_sector_batch_max_size
                    == other.policy.pre_commit_sector_batch_max_size
                && self.policy.prove_replica_updates_max_size
                    == other.policy.prove_replica_updates_max_size
                && self.policy.expired_pre_commit_clean_up_delay
                    == other.policy.expired_pre_commit_clean_up_delay
                && self.policy.wpost_proving_period == other.policy.wpost_proving_period
                && self.policy.wpost_challenge_window == other.policy.wpost_challenge_window
                && self.policy.wpost_period_deadlines == other.policy.wpost_period_deadlines
                && self.policy.wpost_max_chain_commit_age
                    == other.policy.wpost_max_chain_commit_age
                && self.policy.wpost_dispute_window == other.policy.wpost_dispute_window
                && self.policy.sectors_max == other.policy.sectors_max
                && self.policy.max_partitions_per_deadline
                    == other.policy.max_partitions_per_deadline
                && self.policy.max_control_addresses == other.policy.max_control_addresses
                && self.policy.max_peer_id_length == other.policy.max_peer_id_length
                && self.policy.max_multiaddr_data == other.policy.max_multiaddr_data
                && self.policy.addressed_partitions_max == other.policy.addressed_partitions_max
                && self.policy.declarations_max == other.policy.declarations_max
                && self.policy.addressed_sectors_max == other.policy.addressed_sectors_max
                && self.policy.max_pre_commit_randomness_lookback
                    == other.policy.max_pre_commit_randomness_lookback
                && self.policy.pre_commit_challenge_delay
                    == other.policy.pre_commit_challenge_delay
                && self.policy.wpost_challenge_lookback == other.policy.wpost_challenge_lookback
                && self.policy.fault_declaration_cutoff == other.policy.fault_declaration_cutoff
                && self.policy.fault_max_age == other.policy.fault_max_age
                && self.policy.worker_key_change_delay == other.policy.worker_key_change_delay
                && self.policy.min_sector_expiration == other.policy.min_sector_expiration
                && self.policy.max_sector_expiration_extension
                    == other.policy.max_sector_expiration_extension
                && self.policy.deal_limit_denominator == other.policy.deal_limit_denominator
                && self.policy.consensus_fault_ineligibility_duration
                    == other.policy.consensus_fault_ineligibility_duration
                && self.policy.new_sectors_per_period_max
                    == other.policy.new_sectors_per_period_max
                && self.policy.chain_finality == other.policy.chain_finality
                && self.policy.valid_post_proof_type == other.policy.valid_post_proof_type
                && self.policy.valid_pre_commit_proof_type
                    == other.policy.valid_pre_commit_proof_type
                && self.policy.minimum_verified_allocation_size
                    == other.policy.minimum_verified_allocation_size
                && self.policy.deal_updates_interval == other.policy.deal_updates_interval
                && self.policy.prov_collateral_percent_supply_num
                    == other.policy.prov_collateral_percent_supply_num
                && self.policy.prov_collateral_percent_supply_denom
                    == other.policy.prov_collateral_percent_supply_denom
                && self.policy.minimum_consensus_power == other.policy.minimum_consensus_power)
    }
}

impl ChainConfig {
    pub fn calibnet() -> Self {
        use calibnet::*;
        Self {
            name: "calibnet".to_string(),
            genesis_cid: Some(GENESIS_CID.to_owned()),
            bootstrap_peers: DEFAULT_BOOTSTRAP.iter().map(|x| x.to_string()).collect(),
            block_delay_secs: EPOCH_DURATION_SECONDS as u64,
            height_infos: HEIGHT_INFOS.to_vec(),
            policy: Policy {
                valid_post_proof_type: HashSet::<RegisteredPoStProof>::from([
                    RegisteredPoStProof::StackedDRGWindow32GiBV1,
                    RegisteredPoStProof::StackedDRGWindow64GiBV1,
                ]),
                valid_pre_commit_proof_type: HashSet::<RegisteredSealProof>::from([
                    RegisteredSealProof::StackedDRG32GiBV1P1,
                    RegisteredSealProof::StackedDRG64GiBV1P1,
                ]),
                minimum_consensus_power: StoragePower::from(MINIMUM_CONSENSUS_POWER),
                ..Policy::default()
            },
        }
    }

    pub fn network_version(&self, epoch: ChainEpoch) -> NetworkVersion {
        let height = sort_by_epoch(&self.height_infos)
            .iter()
            .rev()
            .find(|info| epoch > info.epoch)
            .map(|info| info.height)
            .unwrap_or(Height::Breeze);

        From::from(height)
    }

    pub async fn get_beacon_schedule(
        &self,
        genesis_ts: u64,
    ) -> Result<BeaconSchedule<DrandBeacon>, anyhow::Error> {
        let ds_iter = if self.name == "calibnet" {
            calibnet::DRAND_SCHEDULE.iter()
        } else {
            mainnet::DRAND_SCHEDULE.iter()
        };
        let mut points = BeaconSchedule::with_capacity(ds_iter.len());
        for dc in ds_iter {
            points.0.push(BeaconPoint {
                height: dc.height,
                beacon: Arc::new(
                    DrandBeacon::new(genesis_ts, self.block_delay_secs, dc.config).await?,
                ),
            });
        }
        Ok(points)
    }

    pub fn epoch(&self, height: Height) -> ChainEpoch {
        sort_by_epoch(&self.height_infos)
            .iter()
            .find(|info| height == info.height)
            .map(|info| info.epoch)
            .unwrap_or(0)
    }

    pub fn genesis_bytes(&self) -> Option<&[u8]> {
        match self.name.as_ref() {
            "mainnet" => {
                use mainnet::DEFAULT_GENESIS;
                Some(DEFAULT_GENESIS)
            }
            "calibnet" => {
                use calibnet::DEFAULT_GENESIS;
                Some(DEFAULT_GENESIS)
            }
            _ => None,
        }
    }
}

impl Default for ChainConfig {
    fn default() -> Self {
        use mainnet::*;
        Self {
            name: "mainnet".to_string(),
            genesis_cid: Some(GENESIS_CID.to_owned()),
            bootstrap_peers: DEFAULT_BOOTSTRAP.iter().map(|x| x.to_string()).collect(),
            block_delay_secs: EPOCH_DURATION_SECONDS as u64,
            height_infos: HEIGHT_INFOS.to_vec(),
            policy: Policy {
                valid_post_proof_type: HashSet::<RegisteredPoStProof>::from([
                    RegisteredPoStProof::StackedDRGWindow32GiBV1,
                    RegisteredPoStProof::StackedDRGWindow64GiBV1,
                ]),
                valid_pre_commit_proof_type: HashSet::<RegisteredSealProof>::from([
                    RegisteredSealProof::StackedDRG32GiBV1P1,
                    RegisteredSealProof::StackedDRG64GiBV1P1,
                ]),
                ..Policy::default()
            },
        }
    }
}

fn default_policy() -> Policy {
    Policy::default()
}

mod serde_policy {
    use crate::*;
    use forest_json::bigint::json as bigint_json;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    struct PolicySerDe {
        max_aggregated_sectors: u64,
        min_aggregated_sectors: u64,
        max_aggregated_proof_size: usize,
        max_replica_update_proof_size: usize,

        pre_commit_sector_batch_max_size: usize,
        prove_replica_updates_max_size: usize,

        expired_pre_commit_clean_up_delay: i64,

        wpost_proving_period: ChainEpoch,
        wpost_challenge_window: ChainEpoch,
        wpost_period_deadlines: u64,
        wpost_max_chain_commit_age: ChainEpoch,
        wpost_dispute_window: ChainEpoch,

        sectors_max: usize,
        max_partitions_per_deadline: u64,
        max_control_addresses: usize,
        max_peer_id_length: usize,
        max_multiaddr_data: usize,
        addressed_partitions_max: u64,
        declarations_max: u64,
        addressed_sectors_max: u64,
        max_pre_commit_randomness_lookback: ChainEpoch,
        pre_commit_challenge_delay: ChainEpoch,
        wpost_challenge_lookback: ChainEpoch,
        fault_declaration_cutoff: ChainEpoch,
        fault_max_age: ChainEpoch,
        worker_key_change_delay: ChainEpoch,
        min_sector_expiration: i64,
        max_sector_expiration_extension: i64,
        deal_limit_denominator: u64,
        consensus_fault_ineligibility_duration: ChainEpoch,
        new_sectors_per_period_max: usize,
        chain_finality: ChainEpoch,
        valid_post_proof_type: HashSet<RegisteredPoStProof>,
        valid_pre_commit_proof_type: HashSet<RegisteredSealProof>,
        #[serde(with = "bigint_json")]
        minimum_verified_allocation_size: StoragePower,
        deal_updates_interval: i64,
        prov_collateral_percent_supply_num: i64,
        prov_collateral_percent_supply_denom: i64,
        #[serde(with = "bigint_json")]
        minimum_consensus_power: StoragePower,
        end_of_life_claim_drop_period: ChainEpoch,
        market_default_allocation_term_buffer: i64,
        maximum_verified_allocation_expiration: i64,
        maximum_verified_allocation_term: i64,
        minimum_verified_allocation_term: i64,
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Policy, D::Error>
    where
        D: Deserializer<'de>,
    {
        let policy: PolicySerDe = Deserialize::deserialize(deserializer)?;
        Ok(Policy {
            max_aggregated_sectors: policy.max_aggregated_sectors,
            min_aggregated_sectors: policy.min_aggregated_sectors,
            max_aggregated_proof_size: policy.max_aggregated_proof_size,
            max_replica_update_proof_size: policy.max_replica_update_proof_size,

            pre_commit_sector_batch_max_size: policy.pre_commit_sector_batch_max_size,
            prove_replica_updates_max_size: policy.prove_replica_updates_max_size,

            expired_pre_commit_clean_up_delay: policy.expired_pre_commit_clean_up_delay,

            wpost_proving_period: policy.wpost_proving_period,
            wpost_challenge_window: policy.wpost_challenge_window,
            wpost_period_deadlines: policy.wpost_period_deadlines,
            wpost_max_chain_commit_age: policy.wpost_max_chain_commit_age,
            wpost_dispute_window: policy.wpost_dispute_window,

            sectors_max: policy.sectors_max,
            max_partitions_per_deadline: policy.max_partitions_per_deadline,
            max_control_addresses: policy.max_control_addresses,
            max_peer_id_length: policy.max_peer_id_length,
            max_multiaddr_data: policy.max_multiaddr_data,
            addressed_partitions_max: policy.addressed_partitions_max,
            declarations_max: policy.declarations_max,
            addressed_sectors_max: policy.addressed_sectors_max,
            max_pre_commit_randomness_lookback: policy.max_pre_commit_randomness_lookback,
            pre_commit_challenge_delay: policy.pre_commit_challenge_delay,
            wpost_challenge_lookback: policy.wpost_challenge_lookback,
            fault_declaration_cutoff: policy.fault_declaration_cutoff,
            fault_max_age: policy.fault_max_age,
            worker_key_change_delay: policy.worker_key_change_delay,
            min_sector_expiration: policy.min_sector_expiration,
            max_sector_expiration_extension: policy.max_sector_expiration_extension,
            deal_limit_denominator: policy.deal_limit_denominator,
            consensus_fault_ineligibility_duration: policy.consensus_fault_ineligibility_duration,
            new_sectors_per_period_max: policy.new_sectors_per_period_max,
            chain_finality: policy.chain_finality,
            valid_post_proof_type: policy.valid_post_proof_type.clone(),
            valid_pre_commit_proof_type: policy.valid_pre_commit_proof_type.clone(),
            minimum_verified_allocation_size: policy.minimum_verified_allocation_size.clone(),
            deal_updates_interval: policy.deal_updates_interval,
            prov_collateral_percent_supply_num: policy.prov_collateral_percent_supply_num,
            prov_collateral_percent_supply_denom: policy.prov_collateral_percent_supply_denom,
            minimum_consensus_power: policy.minimum_consensus_power,

            end_of_life_claim_drop_period: policy.end_of_life_claim_drop_period,
            market_default_allocation_term_buffer: policy.market_default_allocation_term_buffer,
            maximum_verified_allocation_expiration: policy.maximum_verified_allocation_expiration,
            maximum_verified_allocation_term: policy.maximum_verified_allocation_term,
            minimum_verified_allocation_term: policy.minimum_verified_allocation_term,
        })
    }

    pub fn serialize<S>(policy: &Policy, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PolicySerDe {
            max_aggregated_sectors: policy.max_aggregated_sectors,
            min_aggregated_sectors: policy.min_aggregated_sectors,
            max_aggregated_proof_size: policy.max_aggregated_proof_size,
            max_replica_update_proof_size: policy.max_replica_update_proof_size,

            pre_commit_sector_batch_max_size: policy.pre_commit_sector_batch_max_size,
            prove_replica_updates_max_size: policy.prove_replica_updates_max_size,

            expired_pre_commit_clean_up_delay: policy.expired_pre_commit_clean_up_delay,

            wpost_proving_period: policy.wpost_proving_period,
            wpost_challenge_window: policy.wpost_challenge_window,
            wpost_period_deadlines: policy.wpost_period_deadlines,
            wpost_max_chain_commit_age: policy.wpost_max_chain_commit_age,
            wpost_dispute_window: policy.wpost_dispute_window,

            sectors_max: policy.sectors_max,
            max_partitions_per_deadline: policy.max_partitions_per_deadline,
            max_control_addresses: policy.max_control_addresses,
            max_peer_id_length: policy.max_peer_id_length,
            max_multiaddr_data: policy.max_multiaddr_data,
            addressed_partitions_max: policy.addressed_partitions_max,
            declarations_max: policy.declarations_max,
            addressed_sectors_max: policy.addressed_sectors_max,
            max_pre_commit_randomness_lookback: policy.max_pre_commit_randomness_lookback,
            pre_commit_challenge_delay: policy.pre_commit_challenge_delay,
            wpost_challenge_lookback: policy.wpost_challenge_lookback,
            fault_declaration_cutoff: policy.fault_declaration_cutoff,
            fault_max_age: policy.fault_max_age,
            worker_key_change_delay: policy.worker_key_change_delay,
            min_sector_expiration: policy.min_sector_expiration,
            max_sector_expiration_extension: policy.max_sector_expiration_extension,
            deal_limit_denominator: policy.deal_limit_denominator,
            consensus_fault_ineligibility_duration: policy.consensus_fault_ineligibility_duration,
            new_sectors_per_period_max: policy.new_sectors_per_period_max,
            chain_finality: policy.chain_finality,
            valid_post_proof_type: policy.valid_post_proof_type.clone(),
            valid_pre_commit_proof_type: policy.valid_pre_commit_proof_type.clone(),
            minimum_verified_allocation_size: policy.minimum_verified_allocation_size.clone(),
            deal_updates_interval: policy.deal_updates_interval,
            prov_collateral_percent_supply_num: policy.prov_collateral_percent_supply_num,
            prov_collateral_percent_supply_denom: policy.prov_collateral_percent_supply_denom,
            minimum_consensus_power: policy.minimum_consensus_power.clone(),
            end_of_life_claim_drop_period: policy.end_of_life_claim_drop_period,
            market_default_allocation_term_buffer: policy.market_default_allocation_term_buffer,
            maximum_verified_allocation_expiration: policy.maximum_verified_allocation_expiration,
            maximum_verified_allocation_term: policy.maximum_verified_allocation_term,
            minimum_verified_allocation_term: policy.minimum_verified_allocation_term,
        }
        .serialize(serializer)
    }
}

pub fn default_network_version() -> NetworkVersion {
    NetworkVersion::V1
}

pub mod de_network_version {
    use fvm_shared::version::NetworkVersion;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<NetworkVersion, D::Error>
    where
        D: Deserializer<'de>,
    {
        let version: &str = Deserialize::deserialize(deserializer)?;
        let version = version.to_lowercase();

        match version.as_str() {
            "v0" => Ok(NetworkVersion::V0),
            "v1" => Ok(NetworkVersion::V1),
            "v2" => Ok(NetworkVersion::V2),
            "v3" => Ok(NetworkVersion::V3),
            "v4" => Ok(NetworkVersion::V4),
            "v5" => Ok(NetworkVersion::V5),
            "v6" => Ok(NetworkVersion::V6),
            "v7" => Ok(NetworkVersion::V7),
            "v8" => Ok(NetworkVersion::V8),
            "v9" => Ok(NetworkVersion::V9),
            "v10" => Ok(NetworkVersion::V10),
            "v11" => Ok(NetworkVersion::V11),
            "v12" => Ok(NetworkVersion::V12),
            "v13" => Ok(NetworkVersion::V13),
            "v14" => Ok(NetworkVersion::V14),
            "v15" => Ok(NetworkVersion::V15),
            "v16" => Ok(NetworkVersion::V16),
            "v17" => Ok(NetworkVersion::V17),
            _ => Err(de::Error::custom(&format!(
                "Invalid network version: {}",
                version
            ))),
        }
    }

    pub fn serialize<S>(nv: &NetworkVersion, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let version_string = match nv {
            NetworkVersion::V0 => "V0",
            NetworkVersion::V1 => "V1",
            NetworkVersion::V2 => "V2",
            NetworkVersion::V3 => "V3",
            NetworkVersion::V4 => "V4",
            NetworkVersion::V5 => "V5",
            NetworkVersion::V6 => "V6",
            NetworkVersion::V7 => "V7",
            NetworkVersion::V8 => "V8",
            NetworkVersion::V9 => "V9",
            NetworkVersion::V10 => "V10",
            NetworkVersion::V11 => "V11",
            NetworkVersion::V12 => "V12",
            NetworkVersion::V13 => "V13",
            NetworkVersion::V14 => "V14",
            NetworkVersion::V15 => "V15",
            NetworkVersion::V16 => "V16",
            NetworkVersion::V17 => "V17",
            _ => unimplemented!(),
        }
        .to_string();

        version_string.serialize(serializer)
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use toml::de;

    fn remove_whitespace(s: String) -> String {
        s.chars().filter(|c| !c.is_whitespace()).collect()
    }

    #[test]
    pub fn test_serialize_upgrade_info() {
        let input = r#"
            height = "Breeze"
            version = "V1"
        "#;
        let actual: UpgradeInfo = toml::from_str(input).unwrap();

        let expected = UpgradeInfo {
            height: Height::Breeze,
            version: NetworkVersion::V1,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    pub fn test_deserialize_upgrade_info() {
        let input = UpgradeInfo {
            height: Height::Breeze,
            version: NetworkVersion::V1,
        };

        let actual = toml::to_string(&input).unwrap();

        let expected = r#"
            height = "Breeze"
            version = "V1"
        "#;

        assert_eq!(
            remove_whitespace(actual),
            remove_whitespace(expected.to_string())
        );
    }

    #[test]
    pub fn test_default_network_version_serialization() {
        let input = r#" height = "Breeze" "#;
        let actual: UpgradeInfo = toml::from_str(input).unwrap();

        let expected = UpgradeInfo {
            height: Height::Breeze,
            version: NetworkVersion::V1,
        };

        assert_eq!(actual, expected);
    }

    #[test]
    pub fn test_fails_if_network_version_is_invalid() {
        let input = r#" height = "Cthulhu" "#;
        let actual: Result<UpgradeInfo, de::Error> = toml::from_str(input);
        assert!(actual.is_err())
    }
}

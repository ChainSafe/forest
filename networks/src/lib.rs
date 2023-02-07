// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use fil_actors_runtime::runtime::Policy;
use forest_beacon::{BeaconPoint, BeaconSchedule, DrandBeacon, DrandConfig};
use forest_shim::version::NetworkVersion;
use fvm_shared::clock::{ChainEpoch, EPOCH_DURATION_SECONDS};
use serde::{Deserialize, Serialize};

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
#[derive(Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ChainConfig {
    pub name: String,
    pub genesis_cid: Option<String>,
    pub bootstrap_peers: Vec<String>,
    pub block_delay_secs: u64,
    pub height_infos: Vec<HeightInfo>,
    #[serde(default = "default_policy")]
    pub policy: Policy,
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
            policy: Policy::calibnet(),
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

    pub fn get_beacon_schedule(
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
                beacon: Arc::new(DrandBeacon::new(
                    genesis_ts,
                    self.block_delay_secs,
                    dc.config,
                )?),
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
            "mainnet" => Some(mainnet::DEFAULT_GENESIS),
            "calibnet" => Some(calibnet::DEFAULT_GENESIS),
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
            policy: Policy::mainnet(),
        }
    }
}

// XXX: Dummy default. Will be overwritten later. Wish we could get rid of this.
fn default_policy() -> Policy {
    Policy::mainnet()
}

pub fn default_network_version() -> NetworkVersion {
    NetworkVersion::V1
}

pub mod de_network_version {
    use std::borrow::Cow;

    use forest_shim::version::NetworkVersion;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<NetworkVersion, D::Error>
    where
        D: Deserializer<'de>,
    {
        let version: Cow<str> = Deserialize::deserialize(deserializer)?;
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
                "Invalid network version: {version}"
            ))),
        }
    }

    pub fn serialize<S>(nv: &NetworkVersion, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let version_string = match *nv {
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
    use toml::de;

    use super::*;

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

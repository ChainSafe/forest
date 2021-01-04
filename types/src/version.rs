// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use clock::ChainEpoch;
use encoding::repr::Serialize_repr;

/// V1 network upgrade
pub const UPGRADE_BREEZE_HEIGHT: ChainEpoch = 41280;
/// V2 network upgrade
pub const UPGRADE_SMOKE_HEIGHT: ChainEpoch = 51000;
/// V3 network upgrade
pub const UPGRADE_IGNITION_HEIGHT: ChainEpoch = 94000;
/// V4 network upgrade
pub const UPGRADE_ACTORS_V2_HEIGHT: ChainEpoch = 138720;
/// V5 network upgrade
pub const UPGRADE_TAPE_HEIGHT: ChainEpoch = 140760;
/// v6 network upgrade
pub const UPGRADE_KUMQUAT_HEIGHT: ChainEpoch = 170000;

pub const UPGRADE_LIFTOFF_HEIGHT: i64 = 148888;

pub const NEWEST_NETWORK_VERSION: NetworkVersion = NetworkVersion::V6;

struct Upgrade {
    height: ChainEpoch,
    network: NetworkVersion,
}

const MAINNET_SCHEDULE: [Upgrade; 6] = [
    Upgrade {
        height: UPGRADE_BREEZE_HEIGHT,
        network: NetworkVersion::V1,
    },
    Upgrade {
        height: UPGRADE_SMOKE_HEIGHT,
        network: NetworkVersion::V2,
    },
    Upgrade {
        height: UPGRADE_IGNITION_HEIGHT,
        network: NetworkVersion::V3,
    },
    Upgrade {
        height: UPGRADE_ACTORS_V2_HEIGHT,
        network: NetworkVersion::V4,
    },
    Upgrade {
        height: UPGRADE_TAPE_HEIGHT,
        network: NetworkVersion::V5,
    },
    Upgrade {
        height: UPGRADE_KUMQUAT_HEIGHT,
        network: NetworkVersion::V6,
    },
];

/// Specifies the network version
#[derive(Debug, PartialEq, Clone, Copy, PartialOrd, Serialize_repr)]
#[repr(u32)]
pub enum NetworkVersion {
    /// genesis (specs-actors v0.9.3)
    V0,
    /// breeze (specs-actors v0.9.7)
    V1,
    /// smoke (specs-actors v0.9.8)
    V2,
    /// ignition (specs-actors v0.9.11)
    V3,
    /// actors v2 (specs-actors v2.0.x)
    V4,
    /// tape (increases max prove commit size by 10x)
    V5,
    // kumquat (specs-actors v2.2.0)
    V6,
    /// calico (specs-actors v2.3.2)
    V7,
    /// persian (post-2.3.2 behaviour transition)
    V8,
    /// reserved
    V9,
    /// reserved
    V10,
    /// reserved
    V11,
}

/// this function helps us check if we shoudl be getting the newest network
pub fn use_newest_network() -> bool {
    if UPGRADE_BREEZE_HEIGHT <= 0 && UPGRADE_SMOKE_HEIGHT <= 0 {
        return true;
    }
    false
}

/// Gets network version from epoch using default Mainnet schedule
pub fn get_network_version_default(epoch: ChainEpoch) -> NetworkVersion {
    MAINNET_SCHEDULE
        .iter()
        .filter(|upgrade| epoch > upgrade.height)
        .last()
        .map(|upgrade| upgrade.network)
        .unwrap_or(NetworkVersion::V0)
}

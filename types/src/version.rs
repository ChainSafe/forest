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
/// v7 network upgrade
pub const UPGRADE_CALICO_HEIGHT: ChainEpoch = 265200;
/// v8 network upgrade
pub const UPGRADE_PERSIAN_HEIGHT: ChainEpoch = 272400;
/// v9 network upgrade
pub const UPGRADE_ORANGE_HEIGHT: ChainEpoch = 336458;
/// Remove burn on window PoSt fork
// TODO implement updates for height https://github.com/ChainSafe/forest/issues/905
pub const UPGRADE_CLAUS_HEIGHT: ChainEpoch = 343200;

pub const UPGRADE_LIFTOFF_HEIGHT: i64 = 148888;

struct Upgrade {
    height: ChainEpoch,
    network: NetworkVersion,
}

const MAINNET_SCHEDULE: [Upgrade; 9] = [
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
    Upgrade {
        height: UPGRADE_CALICO_HEIGHT,
        network: NetworkVersion::V7,
    },
    Upgrade {
        height: UPGRADE_PERSIAN_HEIGHT,
        network: NetworkVersion::V8,
    },
    Upgrade {
        height: UPGRADE_ORANGE_HEIGHT,
        network: NetworkVersion::V9,
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
    /// orange
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

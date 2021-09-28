// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{drand::DRAND_MAINNET, DrandPoint};
use clock::{ChainEpoch, EPOCH_DURATION_SECONDS};
use fil_types::NetworkVersion;

/// Default genesis car file bytes.
/// Note: This is only specified in the devnet config so it is compilable.
/// In practice, devnet genesis' should be generated and loaded every time.
pub const DEFAULT_GENESIS: &[u8] = include_bytes!("genesis.car");

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
/// Switching to mainnet network name
pub const UPGRADE_LIFTOFF_HEIGHT: i64 = 148888;
/// V6 network upgrade
pub const UPGRADE_KUMQUAT_HEIGHT: ChainEpoch = 170000;
/// V7 network upgrade
pub const UPGRADE_CALICO_HEIGHT: ChainEpoch = 265200;
/// V8 network upgrade
pub const UPGRADE_PERSIAN_HEIGHT: ChainEpoch = 272400;
/// V9 network upgrade
pub const UPGRADE_ORANGE_HEIGHT: ChainEpoch = 336458;
/// Remove burn on window PoSt fork
pub const UPGRADE_CLAUS_HEIGHT: ChainEpoch = 343200;
/// V10 network upgrade
pub const UPGRADE_ACTORS_V3_HEIGHT: ChainEpoch = 550321;
/// V11 network upgrade
pub const UPGRADE_NORWEGIAN_HEIGHT: ChainEpoch = 665280;
/// V12 network upgrade TODO
pub const UPGRADE_ACTORS_V4_HEIGHT: ChainEpoch = 999999;
/// V13 network upgrade TODO
pub const UPGRADE_HYPERDRIVE_HEIGHT: ChainEpoch = 1000000;

pub const UPGRADE_PLACEHOLDER_HEIGHT: ChainEpoch = 9999999;

/// Current network version for the network
pub const NEWEST_NETWORK_VERSION: NetworkVersion = NetworkVersion::V12;

/// Bootstrap peer ids
pub const DEFAULT_BOOTSTRAP: &[&str] = &[];

lazy_static! {
    pub(super) static ref DRAND_SCHEDULE: [DrandPoint<'static>; 1] = [DrandPoint {
        height: 0,
        config: &*DRAND_MAINNET,
    },];
}

/// Time, in seconds, between each block.
pub const BLOCK_DELAY_SECS: u64 = EPOCH_DURATION_SECONDS as u64;

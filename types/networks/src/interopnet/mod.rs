// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{drand::DRAND_MAINNET, DrandPoint};
use clock::{ChainEpoch, EPOCH_DURATION_SECONDS};
use fil_types::NetworkVersion;

/// Default genesis car file bytes.
pub const DEFAULT_GENESIS: &[u8] = include_bytes!("genesis.car");

/// V1 network upgrade
pub const UPGRADE_BREEZE_HEIGHT: ChainEpoch = -1;
/// V2 network upgrade
pub const UPGRADE_SMOKE_HEIGHT: ChainEpoch = -2;
/// V3 network upgrade
pub const UPGRADE_IGNITION_HEIGHT: ChainEpoch = -3;
/// V4 network upgrade
pub const UPGRADE_ACTORS_V2_HEIGHT: ChainEpoch = 30;
/// V5 network upgrade
pub const UPGRADE_TAPE_HEIGHT: ChainEpoch = 60;
/// Switching to mainnet network name
pub const UPGRADE_LIFTOFF_HEIGHT: i64 = -5;
/// V6 network upgrade
pub const UPGRADE_KUMQUAT_HEIGHT: ChainEpoch = 90;
/// V7 network upgrade
pub const UPGRADE_CALICO_HEIGHT: ChainEpoch = 120;
/// V8 network upgrade
pub const UPGRADE_PERSIAN_HEIGHT: ChainEpoch = 150;
/// V9 network upgrade
pub const UPGRADE_ORANGE_HEIGHT: ChainEpoch = 180;
/// Remove burn on window PoSt fork
pub const UPGRADE_CLAUS_HEIGHT: ChainEpoch = 210;
/// V10 network upgrade height TBD
pub const UPGRADE_ACTORS_V3_HEIGHT: ChainEpoch = 999999999;
pub const UPGRADE_PLACEHOLDER_HEIGHT: ChainEpoch = 9999999;

/// Current network version for the network
pub const NEWEST_NETWORK_VERSION: NetworkVersion = NetworkVersion::V9;

/// Bootstrap peer ids
pub const DEFAULT_BOOTSTRAP: &[&str] = &[
    "/dns4/bootstrap-0.interop.fildev.network/tcp/1347/p2p/12D3KooWQmCzFxEPfEoReafjwiLMqwWsBLWLwbeNyVVm9s6foDwh",
    "/dns4/bootstrap-1.interop.fildev.network/tcp/1347/p2p/12D3KooWL8YeT6dDpfushm4Y1LeZjvG1dRMbs8JUERoF4YvxDqfD",
];

lazy_static! {
    pub(super) static ref DRAND_SCHEDULE: [DrandPoint<'static>; 1] = [DrandPoint {
        height: 0,
        config: &*DRAND_MAINNET,
    },];
}

/// Time, in seconds, between each block.
pub const BLOCK_DELAY_SECS: u64 = EPOCH_DURATION_SECONDS as u64;

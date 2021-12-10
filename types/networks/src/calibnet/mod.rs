// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{
    drand::{DRAND_INCENTINET, DRAND_MAINNET},
    DrandPoint,
};
use clock::{ChainEpoch, EPOCH_DURATION_SECONDS};
use fil_types::NetworkVersion;

/// Default genesis car file bytes.
pub const DEFAULT_GENESIS: &[u8] = include_bytes!("genesis.car");

/// V1 network upgrade
#[cfg(feature = "calibnet")]
pub const UPGRADE_BREEZE_HEIGHT: ChainEpoch = -1;

/// V2 network upgrade
#[cfg(feature = "calibnet")]
pub const UPGRADE_SMOKE_HEIGHT: ChainEpoch = -2;

/// V3 network upgrade
#[cfg(feature = "calibnet")]
pub const UPGRADE_IGNITION_HEIGHT: ChainEpoch = -3;

/// V4 network upgrade
#[cfg(feature = "calibnet")]
pub const UPGRADE_ACTORS_V2_HEIGHT: ChainEpoch = 30;

/// V5 network upgrade
#[cfg(feature = "calibnet")]
pub const UPGRADE_TAPE_HEIGHT: ChainEpoch = 60;

#[cfg(feature = "calibnet")]
pub const UPGRADE_LIFTOFF_HEIGHT: i64 = -5;

/// V6 network upgrade
#[cfg(feature = "calibnet")]
pub const UPGRADE_KUMQUAT_HEIGHT: ChainEpoch = 90;

/// V7 network upgrade
#[cfg(feature = "calibnet")]
pub const UPGRADE_CALICO_HEIGHT: ChainEpoch = 120;

/// V8 network upgrade
#[cfg(feature = "calibnet")]
pub const UPGRADE_PERSIAN_HEIGHT: ChainEpoch = 272400;

/// V9 network upgrade
#[cfg(feature = "calibnet")]
pub const UPGRADE_ORANGE_HEIGHT: ChainEpoch = 300;

/// Remove burn on window PoSt fork
#[cfg(feature = "calibnet")]
pub const UPGRADE_CLAUS_HEIGHT: ChainEpoch = 270;

/// V10 network upgrade
#[cfg(feature = "calibnet")]
pub const UPGRADE_ACTORS_V3_HEIGHT: ChainEpoch = 330;

/// V11 network upgrade
#[cfg(feature = "calibnet")]
pub const UPGRADE_NORWEGIAN_HEIGHT: ChainEpoch = 360;

/// V12 network upgrade
#[cfg(feature = "calibnet")]
pub const UPGRADE_ACTORS_V4_HEIGHT: ChainEpoch = 390;

/// V13 network upgrade
#[cfg(feature = "calibnet")]
pub const UPGRADE_HYPERDRIVE_HEIGHT: ChainEpoch = 420;

/// V14 network update
#[cfg(feature = "calibnet")]
pub const UPGRADE_ACTORS_V6_HEIGHT: ChainEpoch = 312746;

pub const UPGRADE_PLACEHOLDER_HEIGHT: ChainEpoch = 9999999;

/// Current network version for the network
pub const NEWEST_NETWORK_VERSION: NetworkVersion = NetworkVersion::V14;

/// Bootstrap peer ids
pub const DEFAULT_BOOTSTRAP: &[&str] = &[
    "/dns4/bootstrap-0.calibration.fildev.network/tcp/1347/p2p/12D3KooWRLZAseMo9h7fRD6ojn6YYDXHsBSavX5YmjBZ9ngtAEec",
    "/dns4/bootstrap-1.calibration.fildev.network/tcp/1347/p2p/12D3KooWJFtDXgZEQMEkjJPSrbfdvh2xfjVKrXeNFG1t8ioJXAzv",
    "/dns4/bootstrap-2.calibration.fildev.network/tcp/1347/p2p/12D3KooWP1uB9Lo7yCA3S17TD4Y5wStP5Nk7Vqh53m8GsFjkyujD",
    "/dns4/bootstrap-3.calibration.fildev.network/tcp/1347/p2p/12D3KooWLrPM4WPK1YRGPCUwndWcDX8GCYgms3DiuofUmxwvhMCn",
];

lazy_static! {
    pub(super) static ref DRAND_SCHEDULE: [DrandPoint<'static>; 2] = [
        DrandPoint {
            height: 0,
            config: &*DRAND_INCENTINET,
        },
        DrandPoint {
            height: UPGRADE_SMOKE_HEIGHT,
            config: &*DRAND_MAINNET,
        },
    ];
}

/// Time, in seconds, between each block.
pub const BLOCK_DELAY_SECS: u64 = EPOCH_DURATION_SECONDS as u64;

// Copyright 2020 ChainSafe Systems
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
/// V12 network upgrade
pub const UPGRADE_ACTORS_V4_HEIGHT: ChainEpoch = 712320;
/// V13 network upgrade
pub const UPGRADE_HYPERDRIVE_HEIGHT: ChainEpoch = 892800;

pub const UPGRADE_PLACEHOLDER_HEIGHT: ChainEpoch = 9999999;

/// Current network version for the network
pub const NEWEST_NETWORK_VERSION: NetworkVersion = NetworkVersion::V11;

/// Bootstrap peer ids
pub const DEFAULT_BOOTSTRAP: &[&str] = &[
    "/dns4/bootstrap-0.mainnet.filops.net/tcp/1347/p2p/12D3KooWCVe8MmsEMes2FzgTpt9fXtmCY7wrq91GRiaC8PHSCCBj",
    "/dns4/bootstrap-1.mainnet.filops.net/tcp/1347/p2p/12D3KooWCwevHg1yLCvktf2nvLu7L9894mcrJR4MsBCcm4syShVc",
    "/dns4/bootstrap-2.mainnet.filops.net/tcp/1347/p2p/12D3KooWEWVwHGn2yR36gKLozmb4YjDJGerotAPGxmdWZx2nxMC4",
    "/dns4/bootstrap-3.mainnet.filops.net/tcp/1347/p2p/12D3KooWKhgq8c7NQ9iGjbyK7v7phXvG6492HQfiDaGHLHLQjk7R",
    "/dns4/bootstrap-4.mainnet.filops.net/tcp/1347/p2p/12D3KooWL6PsFNPhYftrJzGgF5U18hFoaVhfGk7xwzD8yVrHJ3Uc",
    "/dns4/bootstrap-5.mainnet.filops.net/tcp/1347/p2p/12D3KooWLFynvDQiUpXoHroV1YxKHhPJgysQGH2k3ZGwtWzR4dFH",
    "/dns4/bootstrap-6.mainnet.filops.net/tcp/1347/p2p/12D3KooWP5MwCiqdMETF9ub1P3MbCvQCcfconnYHbWg6sUJcDRQQ",
    "/dns4/bootstrap-7.mainnet.filops.net/tcp/1347/p2p/12D3KooWRs3aY1p3juFjPy8gPN95PEQChm2QKGUCAdcDCC4EBMKf",
    "/dns4/bootstrap-8.mainnet.filops.net/tcp/1347/p2p/12D3KooWScFR7385LTyR4zU1bYdzSiiAb5rnNABfVahPvVSzyTkR",
    "/dns4/lotus-bootstrap.forceup.cn/tcp/41778/p2p/12D3KooWFQsv3nRMUevZNWWsY1Wu6NUzUbawnWU5NcRhgKuJA37C",
    "/dns4/bootstrap-0.starpool.in/tcp/12757/p2p/12D3KooWGHpBMeZbestVEWkfdnC9u7p6uFHXL1n7m1ZBqsEmiUzz",
    "/dns4/bootstrap-1.starpool.in/tcp/12757/p2p/12D3KooWQZrGH1PxSNZPum99M1zNvjNFM33d1AAu5DcvdHptuU7u",
    "/dns4/node.glif.io/tcp/1235/p2p/12D3KooWBF8cpp65hp2u9LK5mh19x67ftAam84z9LsfaquTDSBpt",
    "/dns4/bootstrap-0.ipfsmain.cn/tcp/34721/p2p/12D3KooWQnwEGNqcM2nAcPtRR9rAX8Hrg4k9kJLCHoTR5chJfz6d",
    "/dns4/bootstrap-1.ipfsmain.cn/tcp/34723/p2p/12D3KooWMKxMkD5DMpSWsW7dBddKxKT7L2GgbNuckz9otxvkvByP",
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

// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{
    drand::{DRAND_INCENTINET, DRAND_MAINNET},
    DrandPoint, Height, HeightInfo,
};
use fvm_shared::clock::ChainEpoch;

const SMOKE_HEIGHT: ChainEpoch = 51000;

/// Default genesis car file bytes.
pub const DEFAULT_GENESIS: &[u8] = include_bytes!("genesis.car");

/// Bootstrap peer ids.
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

/// Height epochs.
pub const HEIGHT_INFOS: [HeightInfo; 18] = [
    HeightInfo {
        height: Height::Breeze,
        epoch: 41_280,
    },
    HeightInfo {
        height: Height::Smoke,
        epoch: SMOKE_HEIGHT,
    },
    HeightInfo {
        height: Height::Ignition,
        epoch: 94_000,
    },
    HeightInfo {
        height: Height::ActorsV2,
        epoch: 138_720,
    },
    HeightInfo {
        height: Height::Tape,
        epoch: 140_760,
    },
    HeightInfo {
        height: Height::Liftoff,
        epoch: 148_888,
    },
    HeightInfo {
        height: Height::Kumquat,
        epoch: 170_000,
    },
    HeightInfo {
        height: Height::Calico,
        epoch: 265_200,
    },
    HeightInfo {
        height: Height::Persian,
        epoch: 272_400,
    },
    HeightInfo {
        height: Height::Orange,
        epoch: 336_458,
    },
    HeightInfo {
        height: Height::Claus,
        epoch: 343_200,
    },
    HeightInfo {
        height: Height::Trust,
        epoch: 550_321,
    },
    HeightInfo {
        height: Height::Norwegian,
        epoch: 665_280,
    },
    HeightInfo {
        height: Height::Turbo,
        epoch: 712_320,
    },
    HeightInfo {
        height: Height::Hyperdrive,
        epoch: 892_800,
    },
    HeightInfo {
        height: Height::Chocolate,
        epoch: 1_231_620,
    },
    HeightInfo {
        height: Height::OhSnap,
        epoch: 1_594_680,
    },
    HeightInfo {
        height: Height::Skyr,
        epoch: 1_960_320,
    },
];

lazy_static! {
    pub(super) static ref DRAND_SCHEDULE: [DrandPoint<'static>; 2] = [
        DrandPoint {
            height: 0,
            config: &DRAND_INCENTINET,
        },
        DrandPoint {
            height: SMOKE_HEIGHT,
            config: &DRAND_MAINNET,
        },
    ];
}

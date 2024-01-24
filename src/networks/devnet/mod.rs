// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use once_cell::sync::Lazy;

use super::{
    drand::{DRAND_MAINNET, DRAND_QUICKNET},
    get_upgrade_height, DrandPoint, Height, HeightInfo,
};

// https://github.com/ethereum-lists/chains/blob/6b1e3ccad1cfcaae5aa1ab917960258f0ef1a6b6/_data/chains/eip155-31415926.json
pub const ETH_CHAIN_ID: u64 = 31415926;

/// Height epochs.
/// Environment variable names follow
/// <https://github.com/filecoin-project/lotus/blob/8f73f157933435f5020d7b8f23bee9e4ab71cb1c/build/params_2k.go#L108>
pub static HEIGHT_INFOS: Lazy<[HeightInfo; 22]> = Lazy::new(|| {
    [
        HeightInfo {
            height: Height::Breeze,
            epoch: get_upgrade_height("FOREST_BREEZE_HEIGHT", -50),
            bundle: None,
        },
        HeightInfo {
            height: Height::Smoke,
            epoch: get_upgrade_height("FOREST_SMOKE_HEIGHT", -2),
            bundle: None,
        },
        HeightInfo {
            height: Height::Ignition,
            epoch: get_upgrade_height("FOREST_IGNITION_HEIGHT", -3),
            bundle: None,
        },
        HeightInfo {
            height: Height::ActorsV2,
            epoch: get_upgrade_height("FOREST_ACTORSV2_HEIGHT", -3),
            bundle: None,
        },
        HeightInfo {
            height: Height::Tape,
            epoch: get_upgrade_height("FOREST_TAPE_HEIGHT", -4),
            bundle: None,
        },
        HeightInfo {
            height: Height::Liftoff,
            epoch: get_upgrade_height("FOREST_LIFTOFF_HEIGHT", -6),
            bundle: None,
        },
        HeightInfo {
            height: Height::Kumquat,
            epoch: get_upgrade_height("FOREST_KUMQUAT_HEIGHT", -7),
            bundle: None,
        },
        HeightInfo {
            height: Height::Calico,
            epoch: get_upgrade_height("FOREST_CALICO_HEIGHT", -9),
            bundle: None,
        },
        HeightInfo {
            height: Height::Persian,
            epoch: get_upgrade_height("FOREST_PERSIAN_HEIGHT", -10),
            bundle: None,
        },
        HeightInfo {
            height: Height::Orange,
            epoch: get_upgrade_height("FOREST_ORANGE_HEIGHT", -11),
            bundle: None,
        },
        HeightInfo {
            height: Height::Trust,
            epoch: get_upgrade_height("FOREST_ACTORSV3_HEIGHT", -13),
            bundle: None,
        },
        HeightInfo {
            height: Height::Norwegian,
            epoch: get_upgrade_height("FOREST_NORWEGIAN_HEIGHT", -14),
            bundle: None,
        },
        HeightInfo {
            height: Height::Turbo,
            epoch: get_upgrade_height("FOREST_ACTORSV4_HEIGHT", -15),
            bundle: None,
        },
        HeightInfo {
            height: Height::Hyperdrive,
            epoch: get_upgrade_height("FOREST_HYPERDRIVE_HEIGHT", -16),
            bundle: None,
        },
        HeightInfo {
            height: Height::Chocolate,
            epoch: get_upgrade_height("FOREST_CHOCOLATE_HEIGHT", -17),
            bundle: None,
        },
        HeightInfo {
            height: Height::OhSnap,
            epoch: get_upgrade_height("FOREST_OHSNAP_HEIGHT", -18),
            bundle: None,
        },
        HeightInfo {
            height: Height::Skyr,
            epoch: get_upgrade_height("FOREST_SKYR_HEIGHT", -19),
            bundle: None,
        },
        HeightInfo {
            height: Height::Shark,
            epoch: get_upgrade_height("FOREST_SHARK_HEIGHT", -20),
            bundle: Some(
                Cid::try_from("bafy2bzacedozk3jh2j4nobqotkbofodq4chbrabioxbfrygpldgoxs3zwgggk")
                    .unwrap(),
            ),
        },
        HeightInfo {
            height: Height::Hygge,
            epoch: get_upgrade_height("FOREST_HYGGE_HEIGHT", -21),
            bundle: Some(
                Cid::try_from("bafy2bzacebzz376j5kizfck56366kdz5aut6ktqrvqbi3efa2d4l2o2m653ts")
                    .unwrap(),
            ),
        },
        HeightInfo {
            height: Height::Lightning,
            epoch: get_upgrade_height("FOREST_LIGHTNING_HEIGHT", -22),
            bundle: Some(
                Cid::try_from("bafy2bzaceay35go4xbjb45km6o46e5bib3bi46panhovcbedrynzwmm3drr4i")
                    .unwrap(),
            ),
        },
        HeightInfo {
            height: Height::Thunder,
            epoch: get_upgrade_height("FOREST_THUNDER_HEIGHT", -1),
            bundle: None,
        },
        HeightInfo {
            height: Height::Watermelon,
            epoch: get_upgrade_height("FOREST_WATERMELON_HEIGHT", 200),
            bundle: Some(
                Cid::try_from("bafy2bzaceasjdukhhyjbegpli247vbf5h64f7uvxhhebdihuqsj2mwisdwa6o")
                    .unwrap(),
            ),
        },
    ]
});

pub(super) static DRAND_SCHEDULE: [DrandPoint<'static>; 2] = [
    DrandPoint {
        height: 0,
        config: &DRAND_MAINNET,
    },
    DrandPoint {
        // height is TBD
        height: i64::MAX,
        config: &DRAND_QUICKNET,
    },
];

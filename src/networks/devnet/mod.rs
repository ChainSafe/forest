// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use cid::Cid;
use once_cell::sync::Lazy;

use crate::shim::version::NetworkVersion;

use super::{
    drand::{DRAND_MAINNET, DRAND_QUICKNET},
    get_upgrade_height_from_env, DrandPoint, Height, HeightInfo,
};

// https://github.com/ethereum-lists/chains/blob/6b1e3ccad1cfcaae5aa1ab917960258f0ef1a6b6/_data/chains/eip155-31415926.json
pub const ETH_CHAIN_ID: u64 = 31415926;

pub static GENESIS_NETWORK_VERSION: Lazy<NetworkVersion> = Lazy::new(|| {
    if let Ok(version) = std::env::var("FOREST_GENESIS_NETWORK_VERSION") {
        NetworkVersion::from(
            version
                .parse::<u32>()
                .expect("Invalid genesis network version"),
        )
    } else {
        NetworkVersion::V21
    }
});

/// Height epochs.
/// Environment variable names follow
/// <https://github.com/filecoin-project/lotus/blob/8f73f157933435f5020d7b8f23bee9e4ab71cb1c/build/params_2k.go#L108>
pub static HEIGHT_INFOS: Lazy<HashMap<Height, HeightInfo>> = Lazy::new(|| {
    HashMap::from_iter([
        (
            Height::Breeze,
            HeightInfo {
                epoch: get_upgrade_height_from_env("FOREST_BREEZE_HEIGHT").unwrap_or(-50),
                bundle: None,
            },
        ),
        (
            Height::Smoke,
            HeightInfo {
                epoch: get_upgrade_height_from_env("FOREST_SMOKE_HEIGHT").unwrap_or(-2),
                bundle: None,
            },
        ),
        (
            Height::Ignition,
            HeightInfo {
                epoch: get_upgrade_height_from_env("FOREST_IGNITION_HEIGHT").unwrap_or(-3),
                bundle: None,
            },
        ),
        (
            Height::ActorsV2,
            HeightInfo {
                epoch: get_upgrade_height_from_env("FOREST_ACTORSV2_HEIGHT").unwrap_or(-3),
                bundle: None,
            },
        ),
        (
            Height::Liftoff,
            HeightInfo {
                epoch: get_upgrade_height_from_env("FOREST_LIFTOFF_HEIGHT").unwrap_or(-6),
                bundle: None,
            },
        ),
        (
            Height::Calico,
            HeightInfo {
                epoch: get_upgrade_height_from_env("FOREST_CALICO_HEIGHT").unwrap_or(-9),
                bundle: None,
            },
        ),
        (
            Height::Shark,
            HeightInfo {
                epoch: get_upgrade_height_from_env("FOREST_SHARK_HEIGHT").unwrap_or(-20),
                bundle: Some(
                    Cid::try_from("bafy2bzacedozk3jh2j4nobqotkbofodq4chbrabioxbfrygpldgoxs3zwgggk")
                        .unwrap(),
                ),
            },
        ),
        (
            Height::Hygge,
            HeightInfo {
                epoch: get_upgrade_height_from_env("FOREST_HYGGE_HEIGHT").unwrap_or(-21),
                bundle: Some(
                    Cid::try_from("bafy2bzacebzz376j5kizfck56366kdz5aut6ktqrvqbi3efa2d4l2o2m653ts")
                        .unwrap(),
                ),
            },
        ),
        (
            Height::Lightning,
            HeightInfo {
                epoch: get_upgrade_height_from_env("FOREST_LIGHTNING_HEIGHT").unwrap_or(-22),
                bundle: Some(
                    Cid::try_from("bafy2bzaceay35go4xbjb45km6o46e5bib3bi46panhovcbedrynzwmm3drr4i")
                        .unwrap(),
                ),
            },
        ),
        (
            Height::Thunder,
            HeightInfo {
                epoch: get_upgrade_height_from_env("FOREST_THUNDER_HEIGHT").unwrap_or(-23),
                bundle: None,
            },
        ),
        (
            Height::Watermelon,
            HeightInfo {
                epoch: get_upgrade_height_from_env("FOREST_WATERMELON_HEIGHT").unwrap_or(-1),
                bundle: Some(
                    Cid::try_from("bafy2bzaceasjdukhhyjbegpli247vbf5h64f7uvxhhebdihuqsj2mwisdwa6o")
                        .unwrap(),
                ),
            },
        ),
        (
            Height::Dragon,
            HeightInfo {
                epoch: get_upgrade_height_from_env("FOREST_DRAGON_HEIGHT").unwrap_or(20),
                bundle: Some(
                    Cid::try_from("bafy2bzaceap34qfq4emg4fp3xd7bxtzt7pvkaj37kunqm2ccvttchtlljw7d4")
                        .unwrap(),
                ),
            },
        ),
        (
            Height::DragonFix,
            HeightInfo {
                epoch: get_upgrade_height_from_env("FOREST_DRAGON_FIX_HEIGHT").unwrap_or(30),
                bundle: Some(
                    Cid::try_from("bafy2bzacecn7uxgehrqbcs462ktl2h23u23cmduy2etqj6xrd6tkkja56fna4")
                        .unwrap(),
                ),
            },
        ),
    ])
});

pub(super) static DRAND_SCHEDULE: Lazy<[DrandPoint<'static>; 2]> = Lazy::new(|| {
    [
        DrandPoint {
            height: 0,
            config: &DRAND_MAINNET,
        },
        DrandPoint {
            height: get_upgrade_height_from_env("FOREST_DRAND_QUICKNET_HEIGHT").unwrap_or(i64::MAX),
            config: &DRAND_QUICKNET,
        },
    ]
});

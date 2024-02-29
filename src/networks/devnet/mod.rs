// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use cid::Cid;
use once_cell::sync::Lazy;

use super::{
    drand::{DRAND_MAINNET, DRAND_QUICKNET},
    get_upgrade_height_from_env, DrandPoint, Height, HeightInfo,
};

// https://github.com/ethereum-lists/chains/blob/6b1e3ccad1cfcaae5aa1ab917960258f0ef1a6b6/_data/chains/eip155-31415926.json
pub const ETH_CHAIN_ID: u64 = 31415926;

/// Height epochs.
/// Environment variable names follow
/// <https://github.com/filecoin-project/lotus/blob/8f73f157933435f5020d7b8f23bee9e4ab71cb1c/build/params_2k.go#L108>
pub static HEIGHT_INFOS: Lazy<HashMap<Height, HeightInfo>> = Lazy::new(|| {
    HashMap::from_iter([
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
                    Cid::try_from("bafy2bzacedok4fxofxdwkv42whkkukf3g4jwevui4kk5bw7b5unx4t3tjlrya")
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
            // height is TBD.
            // likely to be `get_upgrade_epoch_by_height(HEIGHT_INFOS.iter(), Height::Pineapple).unwrap()`.
            // remember to remove `#[allow(dead_code)]` from `get_upgrade_epoch_by_height`
            height: get_upgrade_height_from_env("FOREST_DRAND_QUICKNET_HEIGHT").unwrap_or(i64::MAX),
            config: &DRAND_QUICKNET,
        },
    ]
});

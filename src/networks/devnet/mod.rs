// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use cid::Cid;
use once_cell::sync::Lazy;

use crate::{make_height, shim::version::NetworkVersion};

use super::{
    drand::{DRAND_MAINNET, DRAND_QUICKNET},
    get_upgrade_height_from_env, DrandPoint, Height, HeightInfo,
};

// https://github.com/ethereum-lists/chains/blob/6b1e3ccad1cfcaae5aa1ab917960258f0ef1a6b6/_data/chains/eip155-31415926.json
pub const ETH_CHAIN_ID: u64 = 31415926;

pub const BREEZE_GAS_TAMPING_DURATION: i64 = 0;

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
        make_height!(
            Breeze,
            get_upgrade_height_from_env("FOREST_BREEZE_HEIGHT").unwrap_or(-50)
        ),
        make_height!(
            Smoke,
            get_upgrade_height_from_env("FOREST_SMOKE_HEIGHT").unwrap_or(-2)
        ),
        make_height!(
            Ignition,
            get_upgrade_height_from_env("FOREST_IGNITION_HEIGHT").unwrap_or(-3)
        ),
        make_height!(
            Refuel,
            get_upgrade_height_from_env("FOREST_REFUEL_HEIGHT").unwrap_or(-4)
        ),
        make_height!(
            Assembly,
            get_upgrade_height_from_env("FOREST_ASSEMBLY_HEIGHT").unwrap_or(-5)
        ),
        make_height!(
            Tape,
            get_upgrade_height_from_env("FOREST_TAPE_HEIGHT").unwrap_or(-6)
        ),
        make_height!(
            Liftoff,
            get_upgrade_height_from_env("FOREST_LIFTOFF_HEIGHT").unwrap_or(-7)
        ),
        make_height!(
            Kumquat,
            get_upgrade_height_from_env("FOREST_KUMQUAT_HEIGHT").unwrap_or(-8)
        ),
        make_height!(
            Calico,
            get_upgrade_height_from_env("FOREST_CALICO_HEIGHT").unwrap_or(-9)
        ),
        make_height!(
            Persian,
            get_upgrade_height_from_env("FOREST_PERSIAN_HEIGHT").unwrap_or(-10)
        ),
        make_height!(
            Claus,
            get_upgrade_height_from_env("FOREST_CLAUS_HEIGHT").unwrap_or(-11)
        ),
        make_height!(
            Orange,
            get_upgrade_height_from_env("FOREST_ORANGE_HEIGHT").unwrap_or(-12)
        ),
        make_height!(
            Trust,
            get_upgrade_height_from_env("FOREST_TRUST_HEIGHT").unwrap_or(-13)
        ),
        make_height!(
            Norwegian,
            get_upgrade_height_from_env("FOREST_NORWEGIAN_HEIGHT").unwrap_or(-14)
        ),
        make_height!(
            Turbo,
            get_upgrade_height_from_env("FOREST_TURBO_HEIGHT").unwrap_or(-15)
        ),
        make_height!(
            Hyperdrive,
            get_upgrade_height_from_env("FOREST_HYPERDRIVE_HEIGHT").unwrap_or(-16)
        ),
        make_height!(
            Chocolate,
            get_upgrade_height_from_env("FOREST_CHOCOLATE_HEIGHT").unwrap_or(-17)
        ),
        make_height!(
            OhSnap,
            get_upgrade_height_from_env("FOREST_OHSNAP_HEIGHT").unwrap_or(-18)
        ),
        make_height!(
            Skyr,
            get_upgrade_height_from_env("FOREST_SKYR_HEIGHT").unwrap_or(-19)
        ),
        make_height!(
            Shark,
            get_upgrade_height_from_env("FOREST_SHARK_HEIGHT").unwrap_or(-20),
            "bafy2bzacedozk3jh2j4nobqotkbofodq4chbrabioxbfrygpldgoxs3zwgggk"
        ),
        make_height!(
            Hygge,
            get_upgrade_height_from_env("FOREST_HYGGE_HEIGHT").unwrap_or(-21),
            "bafy2bzacebzz376j5kizfck56366kdz5aut6ktqrvqbi3efa2d4l2o2m653ts"
        ),
        make_height!(
            Lightning,
            get_upgrade_height_from_env("FOREST_LIGHTNING_HEIGHT").unwrap_or(-22),
            "bafy2bzaceay35go4xbjb45km6o46e5bib3bi46panhovcbedrynzwmm3drr4i"
        ),
        make_height!(
            Thunder,
            get_upgrade_height_from_env("FOREST_THUNDER_HEIGHT").unwrap_or(-23)
        ),
        make_height!(
            Watermelon,
            get_upgrade_height_from_env("FOREST_WATERMELON_HEIGHT").unwrap_or(-1),
            "bafy2bzaceasjdukhhyjbegpli247vbf5h64f7uvxhhebdihuqsj2mwisdwa6o"
        ),
        make_height!(
            Dragon,
            get_upgrade_height_from_env("FOREST_DRAGON_HEIGHT").unwrap_or(20),
            "bafy2bzacecn7uxgehrqbcs462ktl2h23u23cmduy2etqj6xrd6tkkja56fna4"
        ),
        make_height!(
            Phoenix,
            get_upgrade_height_from_env("FOREST_DRAND_QUICKNET_HEIGHT").unwrap_or(i64::MAX)
        ),
        make_height!(
            Aussie,
            get_upgrade_height_from_env("FOREST_AUSSIE_HEIGHT").unwrap_or(9999999999)
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

/// Creates a new devnet policy with the given version.
/// Works with `v10` onward.
#[macro_export]
macro_rules! make_devnet_policy {
    (v11) => {
        fil_actors_shared::v11::runtime::Policy {
            minimum_consensus_power: 2040.into(),
            minimum_verified_allocation_size: 256.into(),
            pre_commit_challenge_delay: 10,
            valid_pre_commit_proof_type: {
                use $crate::shim::sector::RegisteredSealProofV3;
                let mut proofs = fil_actors_shared::v11::runtime::ProofSet::default();
                proofs.insert(RegisteredSealProofV3::StackedDRG2KiBV1P1);
                proofs.insert(RegisteredSealProofV3::StackedDRG8MiBV1P1);
                proofs
            },
            valid_post_proof_type: {
                use $crate::shim::sector::RegisteredPoStProofV3;
                let mut proofs = fil_actors_shared::v11::runtime::ProofSet::default();
                proofs.insert(RegisteredPoStProofV3::StackedDRGWindow2KiBV1);
                proofs.insert(RegisteredPoStProofV3::StackedDRGWindow2KiBV1P1);
                proofs.insert(RegisteredPoStProofV3::StackedDRGWindow8MiBV1);
                proofs.insert(RegisteredPoStProofV3::StackedDRGWindow8MiBV1P1);
                proofs
            },
            ..Default::default()
        }
    };
    ($version:tt) => {
        fil_actors_shared::$version::runtime::Policy {
            minimum_consensus_power: 2040.into(),
            minimum_verified_allocation_size: 256.into(),
            pre_commit_challenge_delay: 10,
            valid_pre_commit_proof_type: {
                let mut proofs = fil_actors_shared::$version::runtime::ProofSet::default();
                proofs.insert(RegisteredSealProofV3::StackedDRG2KiBV1P1);
                proofs.insert(RegisteredSealProofV3::StackedDRG2KiBV1P1_Feat_SyntheticPoRep);
                proofs.insert(RegisteredSealProofV3::StackedDRG8MiBV1P1);
                proofs.insert(RegisteredSealProofV3::StackedDRG8MiBV1P1_Feat_SyntheticPoRep);
                proofs
            },
            valid_post_proof_type: {
                let mut proofs = fil_actors_shared::$version::runtime::ProofSet::default();
                proofs.insert(RegisteredPoStProofV3::StackedDRGWindow2KiBV1P1);
                proofs.insert(RegisteredPoStProofV3::StackedDRGWindow8MiBV1P1);
                proofs
            },
            ..Default::default()
        }
    };
}

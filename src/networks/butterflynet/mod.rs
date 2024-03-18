// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use cid::Cid;
use libp2p::Multiaddr;
use once_cell::sync::Lazy;
use std::str::FromStr;
use url::Url;

use crate::{db::SettingsStore, utils::net::http_get};

use super::{
    drand::{DRAND_MAINNET, DRAND_QUICKNET},
    get_upgrade_height_from_env, parse_bootstrap_peers, DrandPoint, Height, HeightInfo,
};

/// Fetches the genesis CAR from the local database or downloads it if it does not exist.
/// The result bytes may be compressed.
pub async fn fetch_genesis<DB: SettingsStore>(db: &DB) -> anyhow::Result<Vec<u8>> {
    let genesis_key = format!("BUTTERFLY_GENESIS-{}", &*GENESIS_CID);
    if let Some(genesis) = db.read_bin(&genesis_key)? {
        Ok(genesis)
    } else {
        let response = if let Ok(genesis) = http_get(&GENESIS_URL).await {
            genesis
        } else {
            http_get(&GENESIS_URL_ALT).await?
        };
        let genesis = response.bytes().await?;
        db.write_bin(&genesis_key, &genesis)?;
        Ok(genesis.to_vec())
    }
}

/// Genesis CID
pub static GENESIS_CID: Lazy<Cid> = Lazy::new(|| {
    Cid::from_str("bafy2bzacedz4owzn3irngak4cdv3xndfxyy4qi5hcv7p724anbnt27l7dctoq").unwrap()
});

/// Compressed genesis file. It is compressed with zstd and cuts the download size by 80% (from 10 MB to 2 MB).
static GENESIS_URL: Lazy<Url> = Lazy::new(|| {
    "https://forest-snapshots.fra1.cdn.digitaloceanspaces.com/genesis/butterflynet.car.zst"
        .parse()
        .expect("hard-coded URL must parse")
});

/// Alternative URL for the genesis file. This is hosted on the `lotus` repository and is not
/// compressed.
/// The genesis file does not live on the `master` branch, currently on a draft PR.
/// `<https://github.com/filecoin-project/lotus/pull/11458>`
static GENESIS_URL_ALT: Lazy<Url> = Lazy::new(|| {
    "https://github.com/filecoin-project/lotus/raw/3331a7e52c97e5723fc0c613271a616bb3ccdb39/build/genesis/butterflynet.car".parse().expect("hard-coded URL must parse")
});

pub(crate) const MINIMUM_CONSENSUS_POWER: i64 = 2 << 30;
pub(crate) const MINIMUM_VERIED_ALLOCATION: i64 = 1 << 20;
pub(crate) const PRE_COMMIT_CHALLENGE_DELAY: i64 = 150;

/// Default bootstrap peer ids.
pub static DEFAULT_BOOTSTRAP: Lazy<Vec<Multiaddr>> =
    Lazy::new(|| parse_bootstrap_peers(include_str!("../../../build/bootstrap/butterflynet")));

// https://github.com/ethereum-lists/chains/blob/4731f6713c6fc2bf2ae727388642954a6545b3a9/_data/chains/eip155-314159.json
pub const ETH_CHAIN_ID: u64 = 3141592;

/// Height epochs.
pub static HEIGHT_INFOS: Lazy<HashMap<Height, HeightInfo>> = Lazy::new(|| {
    HashMap::from_iter([
        (
            Height::Watermelon,
            HeightInfo {
                epoch: -1,
                bundle: Some(
                    Cid::try_from("bafy2bzacectxvbk77ntedhztd6sszp2btrtvsmy7lp2ypnrk6yl74zb34t2cq")
                        .unwrap(),
                ),
            },
        ),
        (
            Height::Dragon,
            HeightInfo {
                epoch: 2600,
                bundle: Some(
                    Cid::try_from("bafy2bzaceaqx5xa4cwso24rjiu2ketjlztrqlac6dkyol7tlyuhzrle3zfbos")
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
            height: get_upgrade_height_from_env("FOREST_DRAND_QUICKNET_HEIGHT")
                .unwrap_or(HEIGHT_INFOS.get(&Height::Dragon).unwrap().epoch + 120),
            config: &DRAND_QUICKNET,
        },
    ]
});

/// Creates a new butterfly policy with the given version.
/// Works for `v10` onward.
#[macro_export]
macro_rules! make_butterfly_policy {
    (v10) => {{
        use $crate::networks::butterflynet::*;
        use $crate::shim::sector::{RegisteredPoStProofV3, RegisteredSealProofV3};

        let mut policy = fil_actors_shared::v10::runtime::Policy::mainnet();
        policy.minimum_consensus_power = MINIMUM_CONSENSUS_POWER.into();
        policy.minimum_verified_allocation_size = MINIMUM_VERIED_ALLOCATION.into();
        policy.pre_commit_challenge_delay = PRE_COMMIT_CHALLENGE_DELAY;

        #[allow(clippy::disallowed_types)]
        let allowed_proof_types = std::collections::HashSet::from_iter(vec![
            RegisteredSealProofV3::StackedDRG512MiBV1,
            RegisteredSealProofV3::StackedDRG32GiBV1,
            RegisteredSealProofV3::StackedDRG64GiBV1,
        ]);
        policy.valid_pre_commit_proof_type = allowed_proof_types;
        #[allow(clippy::disallowed_types)]
        let allowed_proof_types = std::collections::HashSet::from_iter(vec![
            RegisteredPoStProofV3::StackedDRGWindow512MiBV1,
            RegisteredPoStProofV3::StackedDRGWindow32GiBV1,
            RegisteredPoStProofV3::StackedDRGWindow64GiBV1,
        ]);
        policy.valid_post_proof_type = allowed_proof_types;
        policy
    }};
    ($version:tt) => {{
        use $crate::networks::butterflynet::*;
        use $crate::shim::sector::{RegisteredPoStProofV3, RegisteredSealProofV3};

        let mut policy = fil_actors_shared::$version::runtime::Policy::mainnet();
        policy.minimum_consensus_power = MINIMUM_CONSENSUS_POWER.into();
        policy.minimum_verified_allocation_size = MINIMUM_VERIED_ALLOCATION.into();
        policy.pre_commit_challenge_delay = PRE_COMMIT_CHALLENGE_DELAY;

        let mut proofs = fil_actors_shared::$version::runtime::ProofSet::default_seal_proofs();
        proofs.insert($crate::shim::sector::RegisteredSealProofV3::StackedDRG512MiBV1);
        proofs.insert(RegisteredSealProofV3::StackedDRG32GiBV1);
        proofs.insert(RegisteredSealProofV3::StackedDRG64GiBV1);
        policy.valid_pre_commit_proof_type = proofs;

        let mut proofs = fil_actors_shared::$version::runtime::ProofSet::default_post_proofs();
        proofs.insert(RegisteredPoStProofV3::StackedDRGWindow512MiBV1);
        proofs.insert(RegisteredPoStProofV3::StackedDRGWindow32GiBV1);
        proofs.insert(RegisteredPoStProofV3::StackedDRGWindow64GiBV1);
        policy.valid_post_proof_type = proofs;
        policy
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_boostrap_list_not_empty() {
        assert!(!DEFAULT_BOOTSTRAP.is_empty());
    }

    #[test]
    fn can_create_butterfly_policy() {
        let v10 = make_butterfly_policy!(v10);
        let v11 = make_butterfly_policy!(v11);
        let v12 = make_butterfly_policy!(v12);
        let v13 = make_butterfly_policy!(v13);

        // basic sanity checks
        assert_eq!(v10.minimum_consensus_power, MINIMUM_CONSENSUS_POWER.into());
        assert_eq!(v11.minimum_consensus_power, MINIMUM_CONSENSUS_POWER.into());
        assert_eq!(v12.minimum_consensus_power, MINIMUM_CONSENSUS_POWER.into());
        assert_eq!(v13.minimum_consensus_power, MINIMUM_CONSENSUS_POWER.into());
    }
}

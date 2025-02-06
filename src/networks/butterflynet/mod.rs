// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use cid::Cid;
use libp2p::Multiaddr;
use once_cell::sync::Lazy;
use std::str::FromStr;
use url::Url;

use crate::{
    db::SettingsStore, eth::EthChainId, make_height, shim::version::NetworkVersion,
    utils::net::http_get,
};

use super::{
    actors_bundle::ACTOR_BUNDLES_METADATA, drand::DRAND_QUICKNET, parse_bootstrap_peers,
    DrandPoint, Height, HeightInfo, NetworkChain,
};

pub const GENESIS_NETWORK_VERSION: NetworkVersion = NetworkVersion::V24;

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
    Cid::from_str("bafy2bzacecm7xklkq3hkc2kgm5wnb5shlxmffino6lzhh7lte5acytb7sssr4").unwrap()
});

/// Compressed genesis file. It is compressed with zstd and cuts the download size by 80% (from 10 MB to 2 MB).
static GENESIS_URL: Lazy<Url> = Lazy::new(|| {
    "https://forest-snapshots.fra1.cdn.digitaloceanspaces.com/genesis/butterflynet-bafy2bzacecm7xklkq3hkc2kgm5wnb5shlxmffino6lzhh7lte5acytb7sssr4.car.zst"
        .parse()
        .expect("hard-coded URL must parse")
});

/// Alternative URL for the genesis file. This is hosted on the `lotus` repository and is not
/// compressed.
/// The genesis file does not live on the `master` branch, currently on `butterfly/v24` branch.
/// `<https://github.com/filecoin-project/lotus/commit/36e6a639fd8411dd69048c95ac478468f2755b8d>`
static GENESIS_URL_ALT: Lazy<Url> = Lazy::new(|| {
    "https://github.com/filecoin-project/lotus/raw/b15b3c40b9649e3bc52aa15968d558aa4514ba6a/build/genesis/butterflynet.car.zst".parse().expect("hard-coded URL must parse")
});

pub(crate) const MINIMUM_CONSENSUS_POWER: i64 = 2 << 30;
pub(crate) const MINIMUM_VERIED_ALLOCATION: i64 = 1 << 20;
pub(crate) const PRE_COMMIT_CHALLENGE_DELAY: i64 = 150;

/// Default bootstrap peer ids.
pub static DEFAULT_BOOTSTRAP: Lazy<Vec<Multiaddr>> =
    Lazy::new(|| parse_bootstrap_peers(include_str!("../../../build/bootstrap/butterflynet")));

// https://github.com/ethereum-lists/chains/blob/4731f6713c6fc2bf2ae727388642954a6545b3a9/_data/chains/eip155-314159.json
pub const ETH_CHAIN_ID: EthChainId = 3141592;

pub const BREEZE_GAS_TAMPING_DURATION: i64 = 120;

/// Height epochs.
pub static HEIGHT_INFOS: Lazy<HashMap<Height, HeightInfo>> = Lazy::new(|| {
    HashMap::from_iter([
        make_height!(Breeze, -50),
        make_height!(Smoke, -2),
        make_height!(Ignition, -3),
        make_height!(Refuel, -4),
        make_height!(Assembly, -5),
        make_height!(Tape, -6),
        make_height!(Liftoff, -7),
        make_height!(Kumquat, -8),
        make_height!(Calico, -9),
        make_height!(Persian, -10),
        make_height!(Claus, -11),
        make_height!(Orange, -12),
        make_height!(Trust, -13),
        make_height!(Norwegian, -14),
        make_height!(Turbo, -15),
        make_height!(Hyperdrive, -16),
        make_height!(Chocolate, -17),
        make_height!(OhSnap, -18),
        make_height!(Skyr, -19),
        make_height!(Shark, -20),
        make_height!(Hygge, -21),
        make_height!(Lightning, -22),
        make_height!(Thunder, -23),
        make_height!(Watermelon, -24),
        make_height!(Dragon, -25),
        make_height!(Phoenix, i64::MIN),
        make_height!(Waffle, -26),
        make_height!(TukTuk, -27, get_bundle_cid("v15.0.0-rc1")),
        make_height!(Teep, 100, get_bundle_cid("v16.0.0-dev1")),
    ])
});

fn get_bundle_cid(version: &str) -> Cid {
    ACTOR_BUNDLES_METADATA
        .get(&(NetworkChain::Butterflynet, version.into()))
        .expect("bundle must be defined")
        .bundle_cid
}

pub(super) static DRAND_SCHEDULE: Lazy<[DrandPoint<'static>; 1]> = Lazy::new(|| {
    [DrandPoint {
        height: 0,
        config: &DRAND_QUICKNET,
    }]
});

/// Creates a new butterfly policy with the given version.
/// Works for `v10` onward.
#[macro_export]
macro_rules! make_butterfly_policy {
    (v10) => {{
        use $crate::networks::butterflynet::*;
        use $crate::shim::sector::{RegisteredPoStProofV3, RegisteredSealProofV3};

        let mut policy = fil_actors_shared::v10::runtime::Policy::default();
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

        let mut policy = fil_actors_shared::$version::runtime::Policy::default();
        policy.minimum_consensus_power = MINIMUM_CONSENSUS_POWER.into();
        policy.minimum_verified_allocation_size = MINIMUM_VERIED_ALLOCATION.into();
        policy.pre_commit_challenge_delay = PRE_COMMIT_CHALLENGE_DELAY;

        let mut proofs = fil_actors_shared::$version::runtime::ProofSet::default();
        proofs.insert(RegisteredSealProofV3::StackedDRG512MiBV1P1);
        proofs.insert(RegisteredSealProofV3::StackedDRG32GiBV1P1);
        proofs.insert(RegisteredSealProofV3::StackedDRG64GiBV1P1);
        proofs.insert(RegisteredSealProofV3::StackedDRG512MiBV1P1_Feat_SyntheticPoRep);
        proofs.insert(RegisteredSealProofV3::StackedDRG32GiBV1P1_Feat_SyntheticPoRep);
        proofs.insert(RegisteredSealProofV3::StackedDRG64GiBV1P1_Feat_SyntheticPoRep);
        policy.valid_pre_commit_proof_type = proofs;

        let mut proofs = fil_actors_shared::$version::runtime::ProofSet::default();
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
        let v14 = make_butterfly_policy!(v14);

        // basic sanity checks
        assert_eq!(v10.minimum_consensus_power, MINIMUM_CONSENSUS_POWER.into());
        assert_eq!(v11.minimum_consensus_power, MINIMUM_CONSENSUS_POWER.into());
        assert_eq!(v12.minimum_consensus_power, MINIMUM_CONSENSUS_POWER.into());
        assert_eq!(v13.minimum_consensus_power, MINIMUM_CONSENSUS_POWER.into());
        assert_eq!(v14.minimum_consensus_power, MINIMUM_CONSENSUS_POWER.into());
    }
}

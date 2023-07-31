// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use libp2p::Multiaddr;
use once_cell::sync::Lazy;
use std::str::FromStr;

use super::{drand::DRAND_MAINNET, parse_bootstrap_peers, DrandPoint, Height, HeightInfo};

/// Default genesis car file bytes.
pub const DEFAULT_GENESIS: &[u8] = include_bytes!("genesis.car");
/// Genesis CID
pub static GENESIS_CID: Lazy<Cid> = Lazy::new(|| {
    Cid::from_str("bafy2bzacecyaggy24wol5ruvs6qm73gjibs2l2iyhcqmvi7r7a4ph7zx3yqd4").unwrap()
});

/// Default bootstrap peer ids.
pub static DEFAULT_BOOTSTRAP: Lazy<Vec<Multiaddr>> =
    Lazy::new(|| parse_bootstrap_peers(include_str!("../../../build/bootstrap/calibnet")));

const LIGHTNING_EPOCH: i64 = 489_094;

// The rollover period is the duration between nv19 and nv20 which both old
// proofs (v1) and the new proofs (v1_1) proofs will be accepted by the
// network.
const LIGHTNING_ROLLOVER_PERIOD: i64 = 3120;

// https://github.com/ethereum-lists/chains/blob/4731f6713c6fc2bf2ae727388642954a6545b3a9/_data/chains/eip155-314159.json
pub const ETH_CHAIN_ID: u64 = 314159;

/// Height epochs.
pub static HEIGHT_INFOS: Lazy<[HeightInfo; 21]> = Lazy::new(|| {
    [
        HeightInfo {
            height: Height::Breeze,
            epoch: -1,
            bundle: None,
        },
        HeightInfo {
            height: Height::Smoke,
            epoch: -2,
            bundle: None,
        },
        HeightInfo {
            height: Height::Ignition,
            epoch: -3,
            bundle: None,
        },
        HeightInfo {
            height: Height::ActorsV2,
            epoch: 30,
            bundle: None,
        },
        HeightInfo {
            height: Height::Tape,
            epoch: 60,
            bundle: None,
        },
        HeightInfo {
            height: Height::Liftoff,
            epoch: -5,
            bundle: None,
        },
        HeightInfo {
            height: Height::Kumquat,
            epoch: 90,
            bundle: None,
        },
        HeightInfo {
            height: Height::Calico,
            epoch: 120,
            bundle: None,
        },
        HeightInfo {
            height: Height::Persian,
            epoch: 130,
            bundle: None,
        },
        HeightInfo {
            height: Height::Orange,
            epoch: 300,
            bundle: None,
        },
        HeightInfo {
            height: Height::Trust,
            epoch: 330,
            bundle: None,
        },
        HeightInfo {
            height: Height::Norwegian,
            epoch: 360,
            bundle: None,
        },
        HeightInfo {
            height: Height::Turbo,
            epoch: 390,
            bundle: None,
        },
        HeightInfo {
            height: Height::Hyperdrive,
            epoch: 420,
            bundle: None,
        },
        HeightInfo {
            height: Height::Chocolate,
            epoch: 450,
            bundle: None,
        },
        HeightInfo {
            height: Height::OhSnap,
            epoch: 480,
            bundle: None,
        },
        HeightInfo {
            height: Height::Skyr,
            epoch: 510,
            bundle: None,
        },
        HeightInfo {
            height: Height::Shark,
            epoch: 16_800,
            bundle: Some(
                Cid::try_from("bafy2bzacedbedgynklc4dgpyxippkxmba2mgtw7ecntoneclsvvl4klqwuyyy")
                    .unwrap(),
            ),
        },
        HeightInfo {
            height: Height::Hygge,
            epoch: 322_354,
            bundle: Some(
                Cid::try_from("bafy2bzaced25ta3j6ygs34roprilbtb3f6mxifyfnm7z7ndquaruxzdq3y7lo")
                    .unwrap(),
            ),
        },
        HeightInfo {
            height: Height::Lightning,
            epoch: LIGHTNING_EPOCH,
            bundle: Some(
                Cid::try_from("bafy2bzacedhuowetjy2h4cxnijz2l64h4mzpk5m256oywp4evarpono3cjhco")
                    .unwrap(),
            ),
        },
        HeightInfo {
            height: Height::Thunder,
            epoch: LIGHTNING_EPOCH + LIGHTNING_ROLLOVER_PERIOD,
            bundle: None,
        },
    ]
});

pub(super) static DRAND_SCHEDULE: [DrandPoint<'static>; 1] = [DrandPoint {
    height: 0,
    config: &DRAND_MAINNET,
}];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_boostrap_list_not_empty() {
        assert!(!DEFAULT_BOOTSTRAP.is_empty());
    }
}

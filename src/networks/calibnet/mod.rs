// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
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
pub static HEIGHT_INFOS: Lazy<HashMap<Height, HeightInfo>> = Lazy::new(|| {
    [
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
        HeightInfo {
            height: Height::Watermelon,
            epoch: 1013134,
            bundle: Some(
                Cid::try_from("bafy2bzacedrunxfqta5skb7q7x32lnp4efz2oq7fn226ffm7fu5iqs62jkmvs")
                    .unwrap(),
            ),
        },
        HeightInfo {
            height: Height::WatermelonFix,
            epoch: 1_070_494,
            bundle: Some(
                Cid::try_from("bafy2bzacebl4w5ptfvuw6746w7ev562idkbf5ppq72e6zub22435ws2rukzru")
                    .unwrap(),
            ),
        },
        HeightInfo {
            height: Height::WatermelonFix2,
            epoch: 1_108_174,
            bundle: Some(
                Cid::try_from("bafy2bzacednzb3pkrfnbfhmoqtb3bc6dgvxszpqklf3qcc7qzcage4ewzxsca")
                    .unwrap(),
            ),
        },
    ]
    .iter()
    .map(|info| (info.height, info.clone()))
    .collect()
});

pub(super) static DRAND_SCHEDULE: Lazy<[DrandPoint<'static>; 1]> = Lazy::new(|| {
    [DrandPoint {
        height: 0,
        config: &DRAND_MAINNET,
    }]
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_boostrap_list_not_empty() {
        assert!(!DEFAULT_BOOTSTRAP.is_empty());
    }
}

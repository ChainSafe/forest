// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains code that is shared between the library and the build script

use cid::Cid;
use once_cell::sync::Lazy;
use reqwest::Url;

#[derive(Debug)]
pub struct ActorBundleInfo {
    pub manifest: Cid,
    pub url: Url,
}

pub static ACTOR_BUNDLES: Lazy<[ActorBundleInfo; 8]> = Lazy::new(|| {
    [
        // calibnet
        ActorBundleInfo{
            manifest: Cid::try_from("bafy2bzacedbedgynklc4dgpyxippkxmba2mgtw7ecntoneclsvvl4klqwuyyy").unwrap(),
            url: Url::parse("https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/builtin-actors/calibnet/Shark.car").unwrap(),
        },
        ActorBundleInfo{
            manifest: Cid::try_from("bafy2bzaced25ta3j6ygs34roprilbtb3f6mxifyfnm7z7ndquaruxzdq3y7lo").unwrap(),
            url: Url::parse("https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/builtin-actors/calibnet/Hygge.car").unwrap(),
        },
        ActorBundleInfo{
            manifest: Cid::try_from("bafy2bzacedhuowetjy2h4cxnijz2l64h4mzpk5m256oywp4evarpono3cjhco").unwrap(),
            url: Url::parse("https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/builtin-actors/calibnet/Lightning.car").unwrap(),
        },
        // devnet
        ActorBundleInfo{
            manifest: Cid::try_from("bafy2bzacebzz376j5kizfck56366kdz5aut6ktqrvqbi3efa2d4l2o2m653ts").unwrap(),
            url: Url::parse("https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/builtin-actors/devnet/Hygge.car").unwrap(),
        },
        ActorBundleInfo{
            manifest: Cid::try_from("bafy2bzaceay35go4xbjb45km6o46e5bib3bi46panhovcbedrynzwmm3drr4i").unwrap(),
            url: Url::parse("https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/builtin-actors/devnet/Lightning.car").unwrap(),
        },
        // mainnet
        ActorBundleInfo{
            manifest: Cid::try_from("bafy2bzaceb6j6666h36xnhksu3ww4kxb6e25niayfgkdnifaqi6m6ooc66i6i").unwrap(),
            url: Url::parse("https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/builtin-actors/mainnet/Shark.car").unwrap(),
        },
        ActorBundleInfo{
            manifest: Cid::try_from("bafy2bzacecsuyf7mmvrhkx2evng5gnz5canlnz2fdlzu2lvcgptiq2pzuovos").unwrap(),
            url: Url::parse("https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/builtin-actors/mainnet/Hygge.car").unwrap(),
        },
        ActorBundleInfo{
            manifest: Cid::try_from("bafy2bzacecnhaiwcrpyjvzl4uv4q3jzoif26okl3m66q3cijp3dfwlcxwztwo").unwrap(),
            url: Url::parse("https://forest-continuous-integration.fra1.cdn.digitaloceanspaces.com/builtin-actors/mainnet/Lightning.car").unwrap(),
        },
    ]
});

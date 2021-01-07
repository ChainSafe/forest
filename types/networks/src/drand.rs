// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use beacon::DrandConfig;

lazy_static! {
    pub(super) static ref DRAND_MAINNET: DrandConfig<'static> = DrandConfig {
        server: "https://api.drand.sh",
        chain_info: serde_json::from_str(r#"{"public_key":"868f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31","period":30,"genesis_time":1595431050,"hash":"8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce","groupHash":"176f93498eac9ca337150b46d21dd58673ea4e3581185f869672e59fa4cb390a"}"#).unwrap()
    };
}

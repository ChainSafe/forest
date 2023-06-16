// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::borrow::Cow;

use forest_beacon::{ChainInfo, DrandConfig, DrandNetwork};

pub(super) static DRAND_MAINNET: DrandConfig<'static> = DrandConfig {
    server: "https://api.drand.sh",
    // Source json: serde_json::from_str(r#"{"public_key":"868f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31","period":30,"genesis_time":1595431050,"hash":"8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce","groupHash":"176f93498eac9ca337150b46d21dd58673ea4e3581185f869672e59fa4cb390a"}"#).unwrap(),
    chain_info:  ChainInfo {
        public_key: Cow::Borrowed("868f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31"),
        period: 30,
        genesis_time: 1595431050,
        hash: Cow::Borrowed("8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce"),
        group_hash: Cow::Borrowed("176f93498eac9ca337150b46d21dd58673ea4e3581185f869672e59fa4cb390a"),
    },
    network_type: DrandNetwork::Mainnet,
};

pub(super) static DRAND_INCENTINET: DrandConfig<'static> = DrandConfig {
    server: "https://pl-us.incentinet.drand.sh",
    // Source json: serde_json::from_str(r#"{"public_key":"8cad0c72c606ab27d36ee06de1d5b2db1faf92e447025ca37575ab3a8aac2eaae83192f846fc9e158bc738423753d000","period":30,"genesis_time":1595873820,"hash":"80c8b872c714f4c00fdd3daa465d5514049f457f01f85a4caf68cdcd394ba039","groupHash":"d9406aaed487f7af71851b4399448e311f2328923d454e971536c05398ce2d9b"}"#).unwrap(),
    chain_info:  ChainInfo {
        public_key: Cow::Borrowed("8cad0c72c606ab27d36ee06de1d5b2db1faf92e447025ca37575ab3a8aac2eaae83192f846fc9e158bc738423753d000"),
        period: 30,
        genesis_time: 1595873820,
        hash: Cow::Borrowed("80c8b872c714f4c00fdd3daa465d5514049f457f01f85a4caf68cdcd394ba039"),
        group_hash: Cow::Borrowed("d9406aaed487f7af71851b4399448e311f2328923d454e971536c05398ce2d9b"),
    },
    network_type: DrandNetwork::Incentinet,
};

// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::beacon::{ChainInfo, DrandConfig, DrandNetwork};
use std::borrow::Cow;
use std::sync::LazyLock;

pub(super) static DRAND_MAINNET: LazyLock<DrandConfig<'static>> = LazyLock::new(|| {
    let default = DrandConfig {
        // https://drand.love/developer/http-api/#public-endpoints
        servers: vec![
            "https://api.drand.sh".try_into().unwrap(),
            "https://api2.drand.sh".try_into().unwrap(),
            "https://api3.drand.sh".try_into().unwrap(),
            "https://drand.cloudflare.com".try_into().unwrap(),
            "https://api.drand.secureweb3.com:6875".try_into().unwrap(),
        ],
        // https://api.drand.sh/8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce/info
        chain_info: ChainInfo {
            public_key: Cow::Borrowed(
                "868f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31",
            ),
            period: 30,
            genesis_time: 1595431050,
            hash: Cow::Borrowed("8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce"),
            group_hash: Cow::Borrowed(
                "176f93498eac9ca337150b46d21dd58673ea4e3581185f869672e59fa4cb390a",
            ),
        },
        network_type: DrandNetwork::Mainnet,
    };
    parse_drand_config_from_env_var("FOREST_DRAND_MAINNET_CONFIG").unwrap_or(default)
});

pub(super) static DRAND_QUICKNET: LazyLock<DrandConfig<'static>> = LazyLock::new(|| {
    let default = DrandConfig {
        // https://drand.love/developer/http-api/#public-endpoints
        servers: vec![
            "https://api.drand.sh".try_into().unwrap(),
            "https://api2.drand.sh".try_into().unwrap(),
            "https://api3.drand.sh".try_into().unwrap(),
            "https://drand.cloudflare.com".try_into().unwrap(),
            "https://api.drand.secureweb3.com:6875".try_into().unwrap(),
        ],
        // https://api.drand.sh/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/info
        chain_info: ChainInfo {
            public_key: Cow::Borrowed(
                "83cf0f2896adee7eb8b5f01fcad3912212c437e0073e911fb90022d3e760183c8c4b450b6a0a6c3ac6a5776a2d1064510d1fec758c921cc22b0e17e63aaf4bcb5ed66304de9cf809bd274ca73bab4af5a6e9c76a4bc09e76eae8991ef5ece45a",
            ),
            period: 3,
            genesis_time: 1692803367,
            hash: Cow::Borrowed("52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971"),
            group_hash: Cow::Borrowed(
                "f477d5c89f21a17c863a7f937c6a6d15859414d2be09cd448d4279af331c5d3e",
            ),
        },
        network_type: DrandNetwork::Quicknet,
    };
    parse_drand_config_from_env_var("FOREST_DRAND_QUICKNET_CONFIG").unwrap_or(default)
});

pub(super) static DRAND_INCENTINET: LazyLock<DrandConfig<'static>> = LazyLock::new(|| {
    let default = DrandConfig {
        // Note: This URL is no longer valid.
        // See <https://github.com/filecoin-project/lotus/pull/10476/files> and its related issues
        servers: vec![],
        // Source json: serde_json::from_str(r#"{"public_key":"8cad0c72c606ab27d36ee06de1d5b2db1faf92e447025ca37575ab3a8aac2eaae83192f846fc9e158bc738423753d000","period":30,"genesis_time":1595873820,"hash":"80c8b872c714f4c00fdd3daa465d5514049f457f01f85a4caf68cdcd394ba039","groupHash":"d9406aaed487f7af71851b4399448e311f2328923d454e971536c05398ce2d9b"}"#).unwrap(),
        chain_info: ChainInfo {
            public_key: Cow::Borrowed(
                "8cad0c72c606ab27d36ee06de1d5b2db1faf92e447025ca37575ab3a8aac2eaae83192f846fc9e158bc738423753d000",
            ),
            period: 30,
            genesis_time: 1595873820,
            hash: Cow::Borrowed("80c8b872c714f4c00fdd3daa465d5514049f457f01f85a4caf68cdcd394ba039"),
            group_hash: Cow::Borrowed(
                "d9406aaed487f7af71851b4399448e311f2328923d454e971536c05398ce2d9b",
            ),
        },
        network_type: DrandNetwork::Incentinet,
    };
    parse_drand_config_from_env_var("FOREST_DRAND_INCENTINET_CONFIG").unwrap_or(default)
});

fn parse_drand_config_from_env_var<'a>(key: &str) -> Option<DrandConfig<'a>> {
    match std::env::var(key) {
        Ok(value) if !value.is_empty() => match serde_json::from_str(&value) {
            Ok(config) => {
                tracing::warn!("overriding drand config with environment variable {key}");
                Some(config)
            }
            Err(error) => {
                tracing::warn!(%error, "failed to parse drand config set by environment variable {key}");
                None
            }
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::*;
    use crate::utils::{RetryArgs, net::global_http_client, retry};
    use std::time::Duration;

    #[tokio::test]
    async fn test_drand_mainnet() {
        test_drand(&DRAND_MAINNET).await
    }

    #[tokio::test]
    async fn test_drand_quicknet() {
        test_drand(&DRAND_QUICKNET).await
    }

    #[tokio::test]
    #[ignore = "server url is no longer valid"]
    async fn test_drand_incentinet() {
        test_drand(&DRAND_INCENTINET).await
    }

    #[test]
    fn test_parse_drand_config_from_env_var() {
        let mut config: DrandConfig<'static> = DRAND_QUICKNET.clone();
        config.servers = vec![config.servers[0].clone()];
        let config_json = serde_json::to_string_pretty(&config).unwrap();
        println!("{config_json}");
        let env_key = "FOREST_DRAND_TEST_CONFIG";
        unsafe { std::env::set_var(env_key, config_json) };
        let parsed = parse_drand_config_from_env_var(env_key);
        assert_eq!(parsed, Some(config));
    }

    async fn test_drand<'a>(config: &'a DrandConfig<'a>) {
        let get_remote_chain_info = |server: &'a Url| async move {
            retry(
                RetryArgs {
                    timeout: Some(Duration::from_secs(15)),
                    ..Default::default()
                },
                || async {
                    let remote_chain_info: ChainInfo = global_http_client()
                        .get(server.join(&format!("{}/info", config.chain_info.hash))?)
                        .send()
                        .await?
                        .error_for_status()?
                        .json()
                        .await?;
                    anyhow::Ok(remote_chain_info)
                },
            )
            .await
        };

        let mut remote_chain_info_list = vec![];
        for server in &config.servers {
            if let Ok(remote_chain_info) = get_remote_chain_info(server).await {
                remote_chain_info_list.push(remote_chain_info);
            }
        }
        assert!(
            !remote_chain_info_list.is_empty(),
            "all drand servers on the list are down"
        );
        assert!(
            remote_chain_info_list
                .iter()
                .all(|i| i == &config.chain_info),
            "some servers on the list serve different networks"
        );
    }
}

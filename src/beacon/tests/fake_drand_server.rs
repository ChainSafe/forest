// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::beacon::{BeaconEntryJson, ChainInfo};
use ahash::HashMap;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use std::borrow::Cow;
use std::sync::Arc;
use url::Url;

/// One `drand` chain served by a [`FakeDrandServer`], keyed by `info.hash` like a real relay.
pub struct FakeDrandChain {
    pub info: ChainInfo<'static>,
    pub entries: Vec<BeaconEntryJson>,
}

impl FakeDrandChain {
    /// `quicknet` chain info and the rounds the beacon tests use.
    pub fn quicknet() -> Self {
        Self {
            // https://api.drand.sh/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/info
            info: ChainInfo {
                public_key: Cow::Borrowed(
                    "83cf0f2896adee7eb8b5f01fcad3912212c437e0073e911fb90022d3e760183c8c4b450b6a0a6c3ac6a5776a2d1064510d1fec758c921cc22b0e17e63aaf4bcb5ed66304de9cf809bd274ca73bab4af5a6e9c76a4bc09e76eae8991ef5ece45a",
                ),
                period: 3,
                genesis_time: 1692803367,
                hash: Cow::Borrowed(
                    "52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971",
                ),
                group_hash: Cow::Borrowed(
                    "f477d5c89f21a17c863a7f937c6a6d15859414d2be09cd448d4279af331c5d3e",
                ),
            },
            entries: vec![
                BeaconEntryJson {
                    round: 1,
                    randomness: "1466a6cd24e327188770752f6134001c64d6efcc590ccc26b721611ad96f165a".into(),
                    signature: "b55e7cb2d5c613ee0b2e28d6750aabbb78c39dcc96bd9d38c2c2e12198df95571de8e8e402a0cc48871c7089a2b3af4b".into(),
                    previous_signature: None,
                },
                BeaconEntryJson {
                    round: 2,
                    randomness: "5782d6987841c654515a0e72b2d1ebb4e741234042c37cb19608ae50d93fb60c".into(),
                    signature: "b6b6a585449b66eb12e875b64fcbab3799861a00e4dbf092d99e969a5eac57dd3f798acf61e705fe4f093db926626807".into(),
                    previous_signature: None,
                },
                BeaconEntryJson {
                    round: 3,
                    randomness: "7ef4621ace1c6da4eb2eee7cd901f81385bca5b189771ec0f08d0d2566dd1a21".into(),
                    signature: "b3fab6df720b68cc47175f2c777e86d84187caab5770906f515ff1099cb01e4deaa027075d860823e49477b93c72bd64".into(),
                    previous_signature: None,
                },
                BeaconEntryJson {
                    round: 30662982,
                    randomness: "ae76da5137c6d0a0d3b50d325948b26cc3ff1f804cd7bbca29b9250c2bfe8d67".into(),
                    signature: "8b3edd0d42a2fa36ac15641a2dea2e4f4895acec2ef72ef39e5b6138f0346fb6b046dde1b4b3d09ff5fe8a6a7acd674a".into(),
                    previous_signature: None,
                },
                BeaconEntryJson {
                    round: 30662990,
                    randomness: "aac361dad26f7e5f5c2460fa905ed6d4e2bf8d21ccb67bbe5100204475b56742".into(),
                    signature: "8b2ebe176d153849d5db7f358a3a21be96f8c5d1c26ed2b530fff4419face8909d77f4ee6a211623e2432c30841411b5".into(),
                    previous_signature: None,
                },
                BeaconEntryJson {
                    round: 30662992,
                    randomness: "b21201bdbe54b1e3ca135bf319bbca22b90b8ae30944c4a6790f74267d074827".into(),
                    signature: "b72a64269e84523a73a87db491505b6f1675dfc7f71c69026efdce3996f59ce2d1b446fc00c13beff7fcfd428053a534".into(),
                    previous_signature: None,
                },
                BeaconEntryJson {
                    round: 30663002,
                    randomness: "2a231554933f6fd70314fa470152710fac3eba10d052cab996e8f30854fa7f55".into(),
                    signature: "b9e7e1e3d7d9cf17a9f4703abfae4c137acfbef1fdb45715a98422c244a499ea381c7fd759851ed8eeb8a03d778959b3".into(),
                    previous_signature: None,
                },
            ],
        }
    }
}

struct Chain {
    info: ChainInfo<'static>,
    rounds: HashMap<u64, BeaconEntryJson>,
}

type Chains = Arc<HashMap<String, Chain>>;

/// Serves the subset of the `drand` HTTP API that [`crate::beacon::DrandBeacon`] uses
/// (`/{hash}/info` and `/{hash}/public/{round}`) from in-memory documents on a random
/// localhost port. Unknown chains and rounds answer `404`, as the public relays do.
pub struct FakeDrandServer {
    url: Url,
}

impl FakeDrandServer {
    pub fn start(chains: Vec<FakeDrandChain>) -> Self {
        let chains: Chains = Arc::new(
            chains
                .into_iter()
                .map(|chain| {
                    let rounds = chain
                        .entries
                        .into_iter()
                        .map(|entry| (entry.round, entry))
                        .collect();
                    let hash = chain.info.hash.to_string();
                    (
                        hash,
                        Chain {
                            info: chain.info,
                            rounds,
                        },
                    )
                })
                .collect(),
        );

        let router = Router::new()
            .route("/{hash}/info", get(info))
            .route("/{hash}/public/{round}", get(public_round))
            .with_state(chains);

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind fake drand server");
        listener
            .set_nonblocking(true)
            .expect("set fake drand listener non-blocking");
        let addr = listener.local_addr().expect("fake drand server address");

        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build fake drand server runtime")
                .block_on(async move {
                    let listener = tokio::net::TcpListener::from_std(listener)
                        .expect("adopt fake drand listener");
                    axum::serve(listener, router)
                        .await
                        .expect("fake drand server exited");
                });
        });

        Self {
            url: Url::parse(&format!("http://{addr}/")).expect("fake drand server url"),
        }
    }

    pub fn url(&self) -> &Url {
        &self.url
    }
}

async fn info(State(chains): State<Chains>, Path(hash): Path<String>) -> Response {
    match chains.get(&hash) {
        Some(chain) => Json(chain.info.clone()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn public_round(
    State(chains): State<Chains>,
    Path((hash, round)): Path<(String, u64)>,
) -> Response {
    match chains.get(&hash).and_then(|chain| chain.rounds.get(&round)) {
        Some(entry) => Json(entry.clone()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::net::global_http_client;

    const HASH: &str = "52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971";

    fn chain_info() -> ChainInfo<'static> {
        ChainInfo {
            public_key: Cow::Borrowed("83cf0f28"),
            period: 3,
            genesis_time: 1692803367,
            hash: Cow::Borrowed(HASH),
            group_hash: Cow::Borrowed("f477d5c8"),
        }
    }

    fn round_entry(round: u64) -> BeaconEntryJson {
        BeaconEntryJson {
            round,
            randomness: format!("aa{round:02x}"),
            signature: format!("bb{round:02x}"),
            previous_signature: None,
        }
    }

    fn start_drand_server() -> FakeDrandServer {
        FakeDrandServer::start(vec![FakeDrandChain {
            info: chain_info(),
            entries: vec![round_entry(1), round_entry(2)],
        }])
    }

    async fn get(server: &FakeDrandServer, path: &str) -> reqwest::Response {
        global_http_client()
            .get(server.url().join(path).unwrap())
            .send()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn serves_chain_info() {
        let server = start_drand_server();
        let resp = get(&server, &format!("{HASH}/info")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.json::<ChainInfo>().await.unwrap(), chain_info());
    }

    #[tokio::test]
    async fn serves_recorded_round_verbatim() {
        let server = start_drand_server();
        let resp = get(&server, &format!("{HASH}/public/2")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.json::<BeaconEntryJson>().await.unwrap(),
            round_entry(2)
        );
    }

    #[tokio::test]
    async fn unknown_is_not_found() {
        let server = start_drand_server();
        let resp = get(&server, &format!("{HASH}/public/3")).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let resp = get(&server, "deadbeef/public/1").await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let resp = get(&server, "deadbeef/info").await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}

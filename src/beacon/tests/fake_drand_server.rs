// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::beacon::ChainInfo;
use ahash::HashMap;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::json;
use std::borrow::Cow;
use std::sync::Arc;
use url::Url;

/// One `drand` chain served by a [`FakeDrandServer`], keyed by `info.hash` like a real relay.
pub struct FakeDrandChain {
    pub info: ChainInfo<'static>,
    pub entries: Vec<serde_json::Value>,
}

impl FakeDrandChain {
    /// `mainnet` chain info and the rounds the beacon tests use.
    pub fn mainnet() -> Self {
        Self {
            // https://api.drand.sh/8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce/info
            info: ChainInfo {
                public_key: Cow::Borrowed(
                    "868f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31",
                ),
                period: 30,
                genesis_time: 1595431050,
                hash: Cow::Borrowed(
                    "8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce",
                ),
                group_hash: Cow::Borrowed(
                    "176f93498eac9ca337150b46d21dd58673ea4e3581185f869672e59fa4cb390a",
                ),
            },
            entries: vec![
                json!({ "round": 1, "randomness": "101297f1ca7dc44ef6088d94ad5fb7ba03455dc33d53ddb412bbc4564ed986ec", "signature": "8d61d9100567de44682506aea1a7a6fa6e5491cd27a0a0ed349ef6910ac5ac20ff7bc3e09d7c046566c9f7f3c6f3b10104990e7cb424998203d8f7de586fb7fa5f60045417a432684f85093b06ca91c769f0e7ca19268375e659c2a2352b4655" }),
                json!({ "round": 2, "randomness": "e8fee7dac6eb2b89df97d631cfccedbada7d5d05495bb546eef462e4145fdf8f", "signature": "aa18facd2d51b616511d542de6f9af8a3b920121401dad1434ed1db4a565f10e04fad8d9b2b4e3e0094364374caafe9b10478bf75650124831509c638b5a36a7a232ec70289f8751a2adb47fc32eb70b57dc81c39d48cbcac9fec46cdfc31663" }),
                json!({ "round": 3, "randomness": "5e0c316703de0d11cc63439a26a5082ce966f4f1e4068cd64b35fee906a0f84b", "signature": "a7b0877eaea7a0222f4c39a2c03434c34f5fe3ea47c533d24b88e5c3053b84775ccb78e984addcb55173f40428513f280cc6e0fccc3c89bb1625c7c0b477deb6faae43fc6ec036f09233bf38da16586b3042dd01a7e9ed97c8bafa343cc6071e" }),
                json!({ "round": 3907446, "randomness": "4958332b0b624013aa168807c7fee10abc13d032a2d50ea6d846ecd54e75e605", "signature": "934d1eb250fec0e5234c11a7e30a8c428a975b500df0deb91a6f6ca57dace8d90705812673e517ad163731f7a2861d1d18cfd60dcca4c93bf01f8ad38279e09cf991d7babe0bd81329daec2702bfb8c6b870fb381e35216528e2e2c0b742c2ba" }),
                json!({ "round": 3907447, "randomness": "77076fd6f14c136e5f6fd54489320cdeffd90318691bc0f4badc654562434aed", "signature": "ac7ad5153605b6a3ec082640989b49e34f554ada33a9d944268213fb2a030cbaf0262c916cfaad866bde80682edeb223129465ae9540cdffd7d85b0180eeba125b16fd1b938c2bbc9bf2597fe20be688b58a615a209f2c6701363228c0682755" }),
            ],
        }
    }

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
                json!({ "round": 1, "randomness": "1466a6cd24e327188770752f6134001c64d6efcc590ccc26b721611ad96f165a", "signature": "b55e7cb2d5c613ee0b2e28d6750aabbb78c39dcc96bd9d38c2c2e12198df95571de8e8e402a0cc48871c7089a2b3af4b" }),
                json!({ "round": 2, "randomness": "5782d6987841c654515a0e72b2d1ebb4e741234042c37cb19608ae50d93fb60c", "signature": "b6b6a585449b66eb12e875b64fcbab3799861a00e4dbf092d99e969a5eac57dd3f798acf61e705fe4f093db926626807" }),
                json!({ "round": 3, "randomness": "7ef4621ace1c6da4eb2eee7cd901f81385bca5b189771ec0f08d0d2566dd1a21", "signature": "b3fab6df720b68cc47175f2c777e86d84187caab5770906f515ff1099cb01e4deaa027075d860823e49477b93c72bd64" }),
                json!({ "round": 30662982, "randomness": "ae76da5137c6d0a0d3b50d325948b26cc3ff1f804cd7bbca29b9250c2bfe8d67", "signature": "8b3edd0d42a2fa36ac15641a2dea2e4f4895acec2ef72ef39e5b6138f0346fb6b046dde1b4b3d09ff5fe8a6a7acd674a" }),
                json!({ "round": 30662990, "randomness": "aac361dad26f7e5f5c2460fa905ed6d4e2bf8d21ccb67bbe5100204475b56742", "signature": "8b2ebe176d153849d5db7f358a3a21be96f8c5d1c26ed2b530fff4419face8909d77f4ee6a211623e2432c30841411b5" }),
                json!({ "round": 30662992, "randomness": "b21201bdbe54b1e3ca135bf319bbca22b90b8ae30944c4a6790f74267d074827", "signature": "b72a64269e84523a73a87db491505b6f1675dfc7f71c69026efdce3996f59ce2d1b446fc00c13beff7fcfd428053a534" }),
                json!({ "round": 30663002, "randomness": "2a231554933f6fd70314fa470152710fac3eba10d052cab996e8f30854fa7f55", "signature": "b9e7e1e3d7d9cf17a9f4703abfae4c137acfbef1fdb45715a98422c244a499ea381c7fd759851ed8eeb8a03d778959b3" }),
            ],
        }
    }
}

struct Chain {
    info: serde_json::Value,
    rounds: HashMap<u64, serde_json::Value>,
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
                        .map(|entry| {
                            let round = entry
                                .get("round")
                                .and_then(serde_json::Value::as_u64)
                                .expect("fake drand entry must carry a numeric `round`");
                            (round, entry)
                        })
                        .collect();
                    let info =
                        serde_json::to_value(&chain.info).expect("ChainInfo serializes to JSON");
                    (chain.info.hash.to_string(), Chain { info, rounds })
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
    use crate::beacon::ChainInfo;
    use crate::utils::net::global_http_client;
    use reqwest::StatusCode;

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

    fn round_json(round: u64) -> serde_json::Value {
        json!({
            "round": round,
            "randomness": format!("aa{round:02x}"),
            "signature": format!("bb{round:02x}"),
        })
    }

    fn start() -> FakeDrandServer {
        FakeDrandServer::start(vec![FakeDrandChain {
            info: chain_info(),
            entries: vec![round_json(1), round_json(2)],
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
        let server: FakeDrandServer = start();
        let resp = get(&server, &format!("{HASH}/info")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.json::<ChainInfo>().await.unwrap(), chain_info());
    }

    #[tokio::test]
    async fn serves_recorded_round_verbatim() {
        let server = start();
        let resp = get(&server, &format!("{HASH}/public/2")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.json::<serde_json::Value>().await.unwrap(),
            round_json(2)
        );
    }

    #[tokio::test]
    async fn unknown_is_not_found() {
        let server = start();
        let resp = get(&server, &format!("{HASH}/public/3")).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let resp = get(&server, "deadbeef/public/1").await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let resp = get(&server, "deadbeef/info").await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}

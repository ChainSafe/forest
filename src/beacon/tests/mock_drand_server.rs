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
use std::sync::Arc;
use url::Url;

/// One `drand` chain served by a [`MockDrandServer`], keyed by `info.hash` like a real relay.
pub struct MockDrandChain {
    pub info: ChainInfo<'static>,
    pub entries: Vec<serde_json::Value>,
}

struct Chain {
    info: serde_json::Value,
    rounds: HashMap<u64, serde_json::Value>,
}

type Chains = Arc<HashMap<String, Chain>>;

/// Serves the subset of the `drand` HTTP API that [`crate::beacon::DrandBeacon`] uses
/// (`/{hash}/info` and `/{hash}/public/{round}`) from in-memory documents on a random
/// localhost port. Unknown chains and rounds answer `404`, as the public relays do.
pub struct MockDrandServer {
    url: Url,
}

impl MockDrandServer {
    pub fn start(chains: Vec<MockDrandChain>) -> Self {
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
                                .expect("mock drand entry must carry a numeric `round`");
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

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind mock drand server");
        listener
            .set_nonblocking(true)
            .expect("set mock drand listener non-blocking");
        let addr = listener.local_addr().expect("mock drand server address");

        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build mock drand server runtime")
                .block_on(async move {
                    let listener = tokio::net::TcpListener::from_std(listener)
                        .expect("adopt mock drand listener");
                    axum::serve(listener, router)
                        .await
                        .expect("mock drand server exited");
                });
        });

        Self { url: Url::parse(&format!("http://{addr}/")).expect("mock drand server url"), }
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
    use serde_json::json;
    use std::borrow::Cow;

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

    fn start() -> MockDrandServer {
        MockDrandServer::start(vec![MockDrandChain {
            info: chain_info(),
            entries: vec![round_json(1), round_json(2)],
        }])
    }

    async fn get(server: &MockDrandServer, path: &str) -> reqwest::Response {
        global_http_client()
            .get(server.url().join(path).unwrap())
            .send()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn serves_chain_info() {
        let server: MockDrandServer = start();
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

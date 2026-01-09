// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::reqwest_resume::get;
use axum::body::Body;
use axum::response::IntoResponse;
use bytes::Bytes;
use futures::stream;
use http_range_header::parse_range_header;
use rand::Rng;
use std::net::{Ipv4Addr, SocketAddr};
use std::ops::Range;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_stream::StreamExt as _;

const CHUNK_LEN: usize = 2048;
// `RANDOM_BYTES` size is arbitrarily chosen. We could use something smaller or bigger here.
// The only constraint is that `CHUNK_LEN < RANDOM_BYTES.len()`.
static RANDOM_BYTES: LazyLock<Bytes> = LazyLock::new(|| {
    let mut rng = crate::utils::rand::forest_rng();
    (0..8192).map(|_| rng.r#gen()).collect()
});

fn get_range(value: &http::HeaderValue) -> Range<usize> {
    let s = std::str::from_utf8(value.as_bytes()).unwrap();
    let parse_ranges = parse_range_header(s).unwrap();
    parse_ranges
        .validate(RANDOM_BYTES.len() as u64)
        .map_or(Range::default(), |range| {
            let start = *range[0].start() as usize;
            // The increment here is to convert into a `std::ops::Range`
            // which has an exclusive upper bound.
            let end = *range[0].end() as usize + 1;
            start..end
        })
}

/// Sends a subset of `RANDOM_BYTES` data on each request. This function will introduce an error
/// to simulate a flaky server by aborting the connection after sending the data.
async fn handle_request(headers: http::HeaderMap) -> impl IntoResponse {
    let range = headers
        .get(http::header::RANGE)
        .map_or(0..CHUNK_LEN, get_range);

    let (status_code, body) = if range.is_empty() {
        (http::StatusCode::RANGE_NOT_SATISFIABLE, Body::empty())
    } else {
        let mut subset = RANDOM_BYTES.slice(range);
        subset.truncate(CHUNK_LEN);
        (
            http::StatusCode::PARTIAL_CONTENT,
            Body::from_stream(
                stream::iter([anyhow::Ok(subset), Err(anyhow::anyhow!("Unexpected EOF"))])
                    .throttle(Duration::from_millis(100)),
            ),
        )
    };

    let response_headers = [(http::header::ACCEPT_RANGES, "bytes")];
    (status_code, response_headers, body)
}

async fn create_listener() -> TcpListener {
    TcpListener::bind(SocketAddr::new(
        Ipv4Addr::LOCALHOST.into(),
        0, /* OS-assigned */
    ))
    .await
    .unwrap()
}

fn create_flaky_server(listener: TcpListener) {
    tokio::task::spawn(async move {
        let app = axum::Router::new().route("/", axum::routing::get(handle_request));
        axum::serve(listener, app.into_make_service())
            .await
            .unwrap()
    });
}

#[tokio::test]
async fn test_resumable_get() {
    let listener = create_listener().await;
    let addr = listener.local_addr().unwrap();
    create_flaky_server(listener);

    let resp = get(reqwest::Url::parse(&format!("http://{addr}")).unwrap())
        .await
        .unwrap();

    let data = resp
        .bytes_stream()
        .map(|item| item.unwrap())
        .collect::<Vec<Bytes>>()
        .await
        .concat();
    assert_eq!(*RANDOM_BYTES, data);
}

#[tokio::test]
async fn test_non_resumable_get() {
    let listener = create_listener().await;
    let addr = listener.local_addr().unwrap();
    create_flaky_server(listener);

    let resp = reqwest::get(reqwest::Url::parse(&format!("http://{addr}")).unwrap())
        .await
        .unwrap();

    let mut stream = resp.bytes_stream();

    let data = stream.next().await.unwrap().unwrap();
    assert!(data.len() <= CHUNK_LEN);
    assert_eq!(RANDOM_BYTES[0..data.len()], data);
    let item = stream.next().await.unwrap();
    let err = item.unwrap_err();
    assert!(err.is_decode(), "{err}");
    assert!(stream.next().await.is_none());
}

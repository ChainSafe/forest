// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::utils::reqwest_resume::get;
use bytes::Bytes;
use const_random::const_random;
use futures::stream::StreamExt;
use http_range_header::parse_range_header;
use hyper::header::{self, HeaderValue};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use std::convert::Infallible;
use std::ops::Range;
use std::time::Duration;
use tokio::time::sleep;

const CHUNK_LEN: usize = 2048;
// `RANDOM_BYTES` size is arbitrarily chosen. We could use something smaller or bigger here.
// The only constraint is that `CHUNK_LEN < RANDOM_BYTES.len()`.
const RANDOM_BYTES: [u8; 8192] = const_random!([u8; 8192]);

fn get_range(value: &HeaderValue) -> Range<usize> {
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
async fn handle_request(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let (mut sender, body) = Body::channel();

    let range = req
        .headers()
        .get(header::RANGE)
        .map_or(0..CHUNK_LEN, get_range);
    tokio::task::spawn(async move {
        let mut subset: Bytes = RANDOM_BYTES[range.clone()].into();
        subset.truncate(CHUNK_LEN);
        sender.send_data(subset).await.unwrap();
        // Abort only if we don't have sent all the data. This will be signaled by an empty range.
        if !range.is_empty() {
            // Sleep to ensure the data is sent before the connection is closed.
            sleep(Duration::from_millis(100)).await;
            // `abort` will close the connection with an error so we can test the
            // resume functionality.
            sender.abort();
        }
    });

    let mut response: Response<_> = Response::new(body);
    response
        .headers_mut()
        .insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    Ok(response)
}

async fn create_flaky_server() -> std::net::SocketAddr {
    let make_svc =
        make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle_request)) });

    // A port number of 0 will request that the OS assigns a port.
    let addr = ([127, 0, 0, 1], 0).into();

    let server = Server::bind(&addr).serve(make_svc);
    let addr = server.local_addr();

    tokio::task::spawn(server);
    addr
}

#[tokio::test]
pub async fn test_resumable_get() {
    let addr = create_flaky_server().await;

    let resp = get(reqwest::Url::parse(&format!("http://{addr}")).unwrap())
        .await
        .unwrap();

    let data = resp
        .bytes_stream()
        .map(|item| item.unwrap())
        .collect::<Vec<Bytes>>()
        .await
        .concat();
    assert_eq!(Bytes::from_static(&RANDOM_BYTES), data);
}

#[tokio::test]
pub async fn test_non_resumable_get() {
    let addr = create_flaky_server().await;

    let resp = reqwest::get(reqwest::Url::parse(&format!("http://{addr}")).unwrap())
        .await
        .unwrap();

    let mut stream = resp.bytes_stream();

    let data = stream.next().await.unwrap().unwrap();
    assert!(data.len() <= CHUNK_LEN);
    assert_eq!(Bytes::from_static(&RANDOM_BYTES[0..data.len()]), data);
    let item = stream.next().await.unwrap();
    let err = item.unwrap_err();
    assert!(err.is_body());
    assert!(stream.next().await.is_none());
}

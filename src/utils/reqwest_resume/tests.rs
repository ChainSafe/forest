// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::utils::reqwest_resume::get;
use bytes::Bytes;
use const_random::const_random;
use futures::stream::StreamExt;
use http_range_header::{parse_range_header, RangeUnsatisfiableError};
use hyper::header::{self, HeaderValue};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use std::convert::Infallible;
use std::time::Duration;
use tokio::time::sleep;

const CHUNK_LEN: usize = 2048;
// `RANDOM_BYTES` size is arbitrarily chosen. We could use something smaller or bigger here.
// The only constraint is that `CHUNK_LEN <= RANDOM_BYTES.len()`.
const RANDOM_BYTES: [u8; 8192] = const_random!([u8; 8192]);

fn extract_range_start(value: &HeaderValue, total_len: usize) -> Option<usize> {
    let s = std::str::from_utf8(value.as_bytes()).unwrap();
    let parse_ranges = parse_range_header(s).unwrap();
    match parse_ranges.validate(total_len as u64) {
        Ok(range) => Some(*range[0].start() as usize),
        Err(err) => {
            assert_eq!(err, RangeUnsatisfiableError::RangeReversed);
            None
        }
    }
}

async fn handle_request(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let (mut sender, body) = Body::channel();

    let start = if let Some(range) = req.headers().get(header::RANGE) {
        extract_range_start(range, RANDOM_BYTES.len())
    } else {
        Some(0)
    };

    if let Some(offset) = start {
        tokio::task::spawn(async move {
            sender
                .send_data(Bytes::copy_from_slice(
                    &RANDOM_BYTES[offset..(offset + CHUNK_LEN)],
                ))
                .await
                .unwrap();
            sleep(Duration::from_millis(100)).await;
            // `abort` will close the connection with an error so we can test the
            // resume functionality
            sender.abort();
        });
    }

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

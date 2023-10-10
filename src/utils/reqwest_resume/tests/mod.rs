// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::utils::reqwest_resume::get;
use bytes::Bytes;
use futures::StreamExt;
use http_range_header::parse_range_header;
use hyper::header::{self, HeaderValue};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use std::convert::Infallible;
use std::time::Duration;
use tokio::time::sleep;

const BUFFER_LEN: usize = 4096 * 2;
const CHUNK_LEN: usize = 4096;

fn extract_range_start(value: &HeaderValue, total_len: usize) -> u64 {
    let s = std::str::from_utf8(value.as_bytes()).unwrap();
    let parse_ranges = parse_range_header(s).unwrap();
    let range = parse_ranges.validate(total_len as u64).unwrap();
    *range[0].start()
}

async fn hello(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let buffer = [b'a'; BUFFER_LEN];

    let (mut sender, body) = Body::channel();

    let body = if let Some(range) = req.headers().get(header::RANGE) {
        let offset = extract_range_start(range, buffer.len());

        // Send the rest of the buffer
        tokio::task::spawn(async move {
            sender
                .send_data(Bytes::copy_from_slice(&buffer[offset as usize..]))
                .await
                .unwrap();
        });
        body
    } else {
        tokio::task::spawn(async move {
            sender
                .send_data(Bytes::copy_from_slice(&buffer[0..CHUNK_LEN]))
                .await
                .unwrap();
            sleep(Duration::from_millis(100)).await;
            // `abort` will close the connection with an error so we can test the
            // resume functionality
            sender.abort();
        });
        body
    };

    let mut response: Response<_> = Response::new(body);
    response
        .headers_mut()
        .insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    Ok(response)
}

async fn create_flaky_server() -> (
    tokio::task::JoinHandle<std::result::Result<(), hyper::Error>>,
    std::net::SocketAddr,
) {
    // For every connection, we must make a `Service` to handle all
    // incoming HTTP requests on said connection.

    let make_svc = make_service_fn(|_conn| {
        // This is the `Service` that will handle the connection.
        // `service_fn` is a helper to convert a function that
        // returns a Response into a `Service`.
        async { Ok::<_, Infallible>(service_fn(hello)) }
    });

    // A port number of 0 will request that the OS assigns a port.
    let addr = ([127, 0, 0, 1], 0).into();

    let server = Server::bind(&addr).serve(make_svc);
    let addr = server.local_addr();

    (tokio::task::spawn(server), addr)
}

#[tokio::test]
pub async fn test_resumable_get() {
    let (_, addr) = create_flaky_server().await;

    let resp = get(reqwest::Url::parse(&format!("http://{addr}")).unwrap())
        .await
        .unwrap();

    let mut stream = resp.bytes_stream();
    let mut read_len = 0;
    while let Some(item) = stream.next().await {
        read_len += item.unwrap().len();
    }
    assert_eq!(read_len, BUFFER_LEN);
}

#[tokio::test]
pub async fn test_non_resumable_get() {
    let (_, addr) = create_flaky_server().await;

    let resp = reqwest::get(reqwest::Url::parse(&format!("http://{addr}")).unwrap())
        .await
        .unwrap();

    let mut stream = resp.bytes_stream();

    let item = stream.next().await.unwrap();
    assert_eq!(item.unwrap().len(), CHUNK_LEN);
    let item = stream.next().await.unwrap();
    let err = item.unwrap_err();
    assert!(err.is_body());
    assert!(stream.next().await.is_none());
}

// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// Copyright 2018-2020 Alec Mocatta
// SPDX-License-Identifier: Apache-2.0, MIT

//! Wrapper that uses the `Range` HTTP header to resume get requests.
//!
//! Most of the code can be attributed to `Alec Mocatta` and comes from the crate
//! <https://crates.io/crates/reqwest_resume/>
//! Some modifications have been done to update the code regarding `tokio` and
//! a change in dependency moving from `hyperx` to `hyper`.

use bytes::Bytes;
use futures::{ready, FutureExt, Stream, TryFutureExt};
use hyper::header::{self, HeaderMap, HeaderValue};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::time::sleep;

/// Extension to [`reqwest::Client`] that provides a method to convert it
pub trait ClientExt {
    /// Convert a [`reqwest::Client`] into a [`reqwest_resume::Client`](Client)
    fn resumable(self) -> Client;
}
impl ClientExt for reqwest::Client {
    fn resumable(self) -> Client {
        Client(self)
    }
}

/// A `Client` to make Requests with.
///
/// See [`reqwest::Client`].
#[derive(Debug)]
pub struct Client(reqwest::Client);
impl Client {
    /// Constructs a new `Client`.
    ///
    /// See [`reqwest::Client::new()`].
    pub fn new() -> Self {
        Self(reqwest::Client::new())
    }
    /// Convenience method to make a `GET` request to a URL.
    ///
    /// See [`reqwest::Client::get()`].
    pub fn get(&self, url: reqwest::Url) -> RequestBuilder {
        // <U: reqwest::IntoUrl>
        RequestBuilder(self.0.clone(), reqwest::Method::GET, url)
    }
}

/// A builder to construct the properties of a Request.
///
/// See [`reqwest::RequestBuilder`].
#[derive(Debug)]
pub struct RequestBuilder(reqwest::Client, reqwest::Method, reqwest::Url);
impl RequestBuilder {
    /// Constructs the Request and sends it the target URL, returning a Response.
    ///
    /// See [`reqwest::RequestBuilder::send()`].
    pub fn send(&mut self) -> impl Future<Output = reqwest::Result<Response>> + Send {
        let (client, method, url) = (self.0.clone(), self.1.clone(), self.2.clone());
        async move {
            let response = loop {
                let builder = client.request(method.clone(), url.clone());
                match builder.send().await {
                    Err(err) if !err.is_builder() && !err.is_redirect() && !err.is_status() => {
                        sleep(Duration::from_secs(1)).await
                    }
                    x => break x?,
                }
            };
            let accept_byte_ranges =
                if let Some(value) = response.headers().get(header::ACCEPT_RANGES) {
                    value.as_bytes() == b"bytes"
                } else {
                    false
                };
            let resp = Response {
                client: client,
                method: method,
                url: url,
                response: response,
                accept_byte_ranges,
                pos: 0,
            };
            Ok(resp)
        }
    }
}

/// A Response to a submitted Request.
///
/// See [`reqwest::Response`].
#[derive(Debug)]
pub struct Response {
    client: reqwest::Client,
    method: reqwest::Method,
    url: reqwest::Url,
    response: reqwest::Response,
    accept_byte_ranges: bool,
    pos: u64,
}
impl Response {
    /// Convert the response into a `Stream` of `Bytes` from the body.
    ///
    /// See [`reqwest::Response::bytes_stream()`].
    pub fn bytes_stream(self) -> impl Stream<Item = reqwest::Result<Bytes>> + Send {
        Decoder {
            client: self.client,
            method: self.method,
            url: self.url,
            decoder: Box::pin(self.response.bytes_stream()),
            accept_byte_ranges: self.accept_byte_ranges,
            pos: self.pos,
        }
    }

    pub fn response(self) -> reqwest::Response {
        self.response
    }
}

struct Decoder {
    client: reqwest::Client,
    method: reqwest::Method,
    url: reqwest::Url,
    decoder: Pin<Box<dyn Stream<Item = reqwest::Result<Bytes>> + Send + Unpin>>,
    accept_byte_ranges: bool,
    pos: u64,
}
impl Stream for Decoder {
    type Item = reqwest::Result<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        loop {
            match ready!(self.decoder.as_mut().poll_next(cx)) {
                Some(Err(err)) => {
                    if !self.accept_byte_ranges {
                        break Poll::Ready(Some(Err(err)));
                    }
                    let builder = self.client.request(self.method.clone(), self.url.clone());
                    let mut headers = HeaderMap::new();
                    let value = HeaderValue::from_str(&std::format!("bytes={}-", self.pos))
                        .expect("invalid ASCII string");
                    headers.insert(header::RANGE, value);
                    let builder = builder.headers(headers);
                    // https://developer.mozilla.org/en-US/docs/Web/HTTP/Range_requests
                    // https://github.com/sdroege/gst-plugin-rs/blob/dcb36832329fde0113a41b80ebdb5efd28ead68d/gst-plugin-http/src/httpsrc.rs
                    self.decoder = Box::pin(
                        Box::pin(sleep(Duration::from_secs(1)))
                            .then(|()| builder.send())
                            .map_ok(reqwest::Response::bytes_stream)
                            .try_flatten_stream(),
                    );
                }
                Some(Ok(n)) => {
                    self.pos += n.len() as u64;
                    break Poll::Ready(Some(Ok(n)));
                }
                None => break Poll::Ready(None),
            }
        }
    }
}

/// Shortcut method to quickly make a GET request.
///
/// See [`reqwest::get`].
pub fn get(url: reqwest::Url) -> impl Future<Output = reqwest::Result<Response>> + Send {
    Client::new().get(url).send()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;
    use std::time::Duration;

    use futures::StreamExt;
    use http_range_header::parse_range_header;
    use hyper::service::{make_service_fn, service_fn};
    use hyper::{Body, Request, Response, Server};

    const BUFFER_LEN: usize = 4096 * 2;

    fn extract_range_start(value: &HeaderValue, total_len: usize) -> u64 {
        let s = std::str::from_utf8(value.as_bytes()).unwrap();
        let parse_ranges = parse_range_header(s).unwrap();
        let range = parse_ranges.validate(total_len as u64).unwrap();
        *range[0].start()
    }

    async fn hello(req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let buffer = [b'a'; BUFFER_LEN];

        let body = if let Some(range) = req.headers().get(header::RANGE) {
            let offset = extract_range_start(&range, buffer.len());
            let (mut sender, body) = Body::channel();

            // Send the rest of the buffer
            let handle = tokio::task::spawn(async move {
                sender
                    .send_data(Bytes::copy_from_slice(&buffer[offset as usize..]))
                    .await
                    .unwrap();
            });
            body
        } else {
            let (mut sender, body) = Body::channel();
            let handle = tokio::task::spawn(async move {
                sender
                    .send_data(Bytes::copy_from_slice(&buffer[0..4096]))
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

    #[tokio::test]
    pub async fn test_resume_get() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // For every connection, we must make a `Service` to handle all
        // incoming HTTP requests on said connection.

        let make_svc = make_service_fn(|_conn| {
            // This is the `Service` that will handle the connection.
            // `service_fn` is a helper to convert a function that
            // returns a Response into a `Service`.
            async { Ok::<_, Infallible>(service_fn(hello)) }
        });

        let addr = ([127, 0, 0, 1], 3000).into();

        let server = Server::bind(&addr).serve(make_svc);

        println!("Listening on http://{}", addr);

        tokio::task::spawn(server);

        let resp = get(reqwest::Url::parse("http://localhost:3000").unwrap()).await?;

        let mut stream = resp.bytes_stream();
        let mut read_len = 0;
        while let Some(item) = stream.next().await {
            read_len += item?.len();
        }
        assert_eq!(read_len, BUFFER_LEN);

        Ok(())
    }
}

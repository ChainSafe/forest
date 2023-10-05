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
                _client: client,
                _method: method,
                _url: url,
                _response: response,
                _accept_byte_ranges: accept_byte_ranges,
                _pos: 0,
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
    _client: reqwest::Client,
    _method: reqwest::Method,
    _url: reqwest::Url,
    _response: reqwest::Response,
    _accept_byte_ranges: bool,
    _pos: u64,
}

impl Response {
    pub fn response(self) -> reqwest::Response {
        self._response
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

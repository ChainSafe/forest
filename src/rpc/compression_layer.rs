// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! HTTP response compression middleware that normalizes the response body
//! back to `jsonrpsee`'s [`HttpBody`] so the layer can be conditionally
//! installed via [`tower::util::option_layer`].
//!
//! Ported from `reth_rpc_layer::compression_layer`.

use std::{
    env,
    future::Future,
    pin::Pin,
    sync::LazyLock,
    task::{Context, Poll},
};

use jsonrpsee::server::{HttpBody, HttpRequest, HttpResponse};
use tower::{Layer, Service};
use tower_http::compression::predicate::SizeAbove;
use tower_http::compression::{Compression, CompressionLayer as TowerCompressionLayer};

const COMPRESS_MIN_BODY_SIZE_VAR: &str = "FOREST_RPC_COMPRESS_MIN_BODY_SIZE";

/// RPC response compression policy, read from [`COMPRESS_MIN_BODY_SIZE_VAR`].
///
/// `None` means no [`CompressionLayer`] is installed at all; `Some(bytes)`
/// means install a layer that compresses responses whose body is at least
/// `bytes`.
///
/// - Any value in `0..=u16::MAX` sets the minimum response size that will
///   be gzip-encoded; smaller responses are sent as-is. Values above
///   `u16::MAX` are clamped because `SizeAbove` is backed by a `u16`.
/// - Any negative integer (e.g. `-1`) disables compression entirely.
/// - Unset defaults to 1 KiB.
pub(crate) static COMPRESS_MIN_BODY_SIZE: LazyLock<Option<u16>> = LazyLock::new(|| {
    parse_compress_min_body_size(env::var(COMPRESS_MIN_BODY_SIZE_VAR).ok().as_deref())
});

/// Interpret a [`COMPRESS_MIN_BODY_SIZE_VAR`] value.
///
/// Returns `None` to signal "compression disabled", `Some(bytes)` for the
/// minimum response size above which compression should be applied.
/// Unset and unparsable values fall back to the 1 KiB default.
/// Values above `u16::MAX` are clamped to `u16::MAX`.
fn parse_compress_min_body_size(raw: Option<&str>) -> Option<u16> {
    // Seems like a sane default, e.g., `erpc` uses 1024 bytes as well.
    // <https://docs.erpc.cloud/config/database/evm-json-rpc-cache#compression>
    const DEFAULT: u16 = 1024;
    let Some(raw) = raw else {
        return Some(DEFAULT);
    };
    // Parse as i128 so any realistically-typable integer lands in one of the
    // defined branches (negative → None, too-large → clamp) rather than
    // silently falling back to DEFAULT just because it didn't fit in i32.
    let Ok(parsed) = raw.parse::<i128>() else {
        tracing::warn!(
            "{COMPRESS_MIN_BODY_SIZE_VAR}={raw:?} is not a valid integer; \
             falling back to default ({DEFAULT} bytes)"
        );
        return Some(DEFAULT);
    };
    if parsed < 0 {
        return None;
    }
    let max = i128::from(u16::MAX);
    if parsed > max {
        tracing::warn!(
            "{COMPRESS_MIN_BODY_SIZE_VAR}={parsed} exceeds the maximum of {max}; \
             clamping to {max} bytes"
        );
    }
    // The prior branches bound `parsed.min(max)` to `[0, u16::MAX]`.
    Some(u16::try_from(parsed.min(max)).expect("bounded above to u16::MAX"))
}

/// Compresses responses with a body above `min_body_size` bytes.
#[derive(Clone)]
pub(crate) struct CompressionLayer {
    inner: TowerCompressionLayer<SizeAbove>,
}

impl CompressionLayer {
    /// Compress responses whose body is at least `min_body_size` bytes.
    pub(crate) fn new(min_body_size: u16) -> Self {
        Self {
            inner: TowerCompressionLayer::new().compress_when(SizeAbove::new(min_body_size)),
        }
    }
}

impl<S> Layer<S> for CompressionLayer {
    type Service = CompressionService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CompressionService {
            inner: self.inner.layer(inner),
        }
    }
}

#[derive(Clone)]
pub(crate) struct CompressionService<S> {
    inner: Compression<S, SizeAbove>,
}

impl<S, ReqBody> Service<HttpRequest<ReqBody>> for CompressionService<S>
where
    S: Service<HttpRequest<ReqBody>, Response = HttpResponse>,
    S::Future: Send + 'static,
{
    type Response = HttpResponse;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: HttpRequest<ReqBody>) -> Self::Future {
        let fut = self.inner.call(req);
        Box::pin(async move {
            // Re-box to match `Identity`'s response body type (see module doc).
            let resp = fut.await?;
            let (parts, compressed_body) = resp.into_parts();
            Ok(Self::Response::from_parts(
                parts,
                HttpBody::new(compressed_body),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::header::{ACCEPT_ENCODING, CONTENT_ENCODING};
    use std::{convert::Infallible, future::ready};

    const TEST_DATA: &str = "cthulhu fhtagn ";
    const REPEAT_COUNT: usize = 1000;

    #[derive(Clone)]
    struct MockService;

    impl Service<HttpRequest> for MockService {
        type Response = HttpResponse;
        type Error = Infallible;
        type Future = std::future::Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _: HttpRequest) -> Self::Future {
            let body = HttpBody::from(TEST_DATA.repeat(REPEAT_COUNT));
            ready(Ok(HttpResponse::builder().body(body).unwrap()))
        }
    }

    async fn body_size(resp: HttpResponse) -> usize {
        let body = axum::body::Body::new(resp.into_body());
        axum::body::to_bytes(body, usize::MAX).await.unwrap().len()
    }

    fn uncompressed_size() -> usize {
        TEST_DATA.repeat(REPEAT_COUNT).len()
    }

    #[tokio::test]
    async fn gzip_compresses_when_requested() {
        let mut svc = CompressionLayer::new(0).layer(MockService);
        let req = HttpRequest::builder()
            .header(ACCEPT_ENCODING, "gzip")
            .body(HttpBody::empty())
            .unwrap();
        let resp = svc.call(req).await.unwrap();
        assert_eq!(resp.headers().get(CONTENT_ENCODING).unwrap(), "gzip");
        assert!(body_size(resp).await < uncompressed_size());
    }

    #[tokio::test]
    async fn passthrough_when_encoding_not_requested() {
        let mut svc = CompressionLayer::new(0).layer(MockService);
        let req = HttpRequest::builder().body(HttpBody::empty()).unwrap();
        let resp = svc.call(req).await.unwrap();
        assert!(resp.headers().get(CONTENT_ENCODING).is_none());
        assert_eq!(body_size(resp).await, uncompressed_size());
    }

    #[tokio::test]
    async fn below_threshold_is_not_compressed() {
        let mut svc = CompressionLayer::new(u16::MAX).layer(MockService);
        let req = HttpRequest::builder()
            .header(ACCEPT_ENCODING, "gzip")
            .body(HttpBody::empty())
            .unwrap();
        let resp = svc.call(req).await.unwrap();
        assert!(resp.headers().get(CONTENT_ENCODING).is_none());
        assert_eq!(body_size(resp).await, uncompressed_size());
    }

    #[test]
    fn parse_defaults_when_unset() {
        assert_eq!(parse_compress_min_body_size(None), Some(1024));
    }

    #[test]
    fn parse_negative_disables() {
        assert_eq!(parse_compress_min_body_size(Some("-1")), None);
        assert_eq!(parse_compress_min_body_size(Some("-999999")), None);
        assert_eq!(parse_compress_min_body_size(Some("-2147483648")), None); // i32::MIN
        // Values below i32::MIN must still disable rather than fall back.
        assert_eq!(
            parse_compress_min_body_size(Some("-9223372036854775808")),
            None
        ); // i64::MIN
    }

    #[test]
    fn parse_accepts_in_range_values() {
        assert_eq!(parse_compress_min_body_size(Some("0")), Some(0));
        assert_eq!(parse_compress_min_body_size(Some("512")), Some(512));
        assert_eq!(parse_compress_min_body_size(Some("1024")), Some(1024));
        assert_eq!(parse_compress_min_body_size(Some("65535")), Some(u16::MAX));
    }

    #[test]
    fn parse_clamps_above_u16_max() {
        assert_eq!(parse_compress_min_body_size(Some("65536")), Some(u16::MAX));
        assert_eq!(
            parse_compress_min_body_size(Some("1000000")),
            Some(u16::MAX)
        );
        assert_eq!(
            parse_compress_min_body_size(Some("2147483647")), // i32::MAX
            Some(u16::MAX)
        );
        // Values above i32::MAX must still clamp rather than fall back.
        assert_eq!(
            parse_compress_min_body_size(Some("99999999999")),
            Some(u16::MAX)
        );
        assert_eq!(
            parse_compress_min_body_size(Some("9223372036854775807")), // i64::MAX
            Some(u16::MAX)
        );
    }

    #[test]
    fn parse_invalid_falls_back_to_default() {
        assert_eq!(parse_compress_min_body_size(Some("")), Some(1024));
        assert_eq!(parse_compress_min_body_size(Some("lots")), Some(1024));
        assert_eq!(parse_compress_min_body_size(Some("1.5")), Some(1024));
    }
}

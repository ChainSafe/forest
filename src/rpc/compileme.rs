// this is a cut-down example from https://github.com/paritytech/jsonrpsee/blob/8a24e2451f662f61d89c727e40efcd06168664dd/examples/examples/http_middleware.rs
// it definitely compiles
// but it doesn't compile in our codebase - so is there a version mismatch somewhere?
// perhaps jsonrpsee hasn't taken e.g hyper 1.0
// - here's the PR for that: https://github.com/paritytech/jsonrpsee/pull/1266
//   - jsonrpsee::server relies on hyper 0.14 - should that even matter for our use-case?

use hyper::body::Bytes;
use hyper::http::HeaderValue;
use hyper::Method;
use jsonrpsee::rpc_params;
use std::iter::once;
use std::net::SocketAddr;
use std::time::Duration;
use tower_http::cors::CorsLayer;
use tower_http::sensitive_headers::SetSensitiveRequestHeadersLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;

use jsonrpsee::core::client::ClientT;
use jsonrpsee::http_client::HttpClientBuilder;
use jsonrpsee::server::{RpcModule, Server};
use jsonrpsee::ws_client::WsClientBuilder;

async fn run_server() -> anyhow::Result<SocketAddr> {
    // let cors = CorsLayer::new()
    //     // Allow `POST` when accessing the resource
    //     .allow_methods([Method::POST])
    //     // Allow requests from any origin
    //     .allow_origin(HeaderValue::from_str("http://example.com").unwrap())
    //     .allow_headers([hyper::header::CONTENT_TYPE]);

    // Custom tower service to handle the RPC requests
    let service_builder = tower::ServiceBuilder::new()
		// Add high level tracing/logging to all requests
		.layer(
			TraceLayer::new_for_http()
				.on_request(
					|request: &hyper::Request<hyper::Body>, _span: &tracing::Span| tracing::info!(request = ?request, "on_request"),
				)
				.on_body_chunk(|chunk: &Bytes, latency: Duration, _: &tracing::Span| {
					tracing::info!(size_bytes = chunk.len(), latency = ?latency, "sending body chunk")
				})
				.make_span_with(DefaultMakeSpan::new().include_headers(true))
				.on_response(DefaultOnResponse::new().include_headers(true).latency_unit(LatencyUnit::Micros)),
		)
		// Mark the `Authorization` request header as sensitive so it doesn't show in logs
		// .layer(SetSensitiveRequestHeadersLayer::new(once(hyper::header::AUTHORIZATION)))
		// .layer(cors)
		.timeout(Duration::from_secs(2));

    let server = Server::builder()
        .set_http_middleware(service_builder)
        .build("127.0.0.1:0".parse::<SocketAddr>()?)
        .await?;

    let addr = server.local_addr()?;

    let mut module = RpcModule::new(());
    module.register_method("say_hello", |_, _| "lo").unwrap();

    let handle = server.start(module); // WRONG

    // In this example we don't care about doing shutdown so let's it run forever.
    // You may use the `ServerHandle` to shut it down or manage it yourself.
    tokio::spawn(handle.stopped());

    Ok(addr)
}

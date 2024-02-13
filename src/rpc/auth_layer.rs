// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use hyper::{Body, Request};
use jsonrpc_v2::RequestObject as JsonRpcRequestObject;

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower::{Layer, Service};

pub struct LogLayer {
    pub target: &'static str,
}

impl<S> Layer<S> for LogLayer {
    type Service = LogService<S>;

    fn layer(&self, service: S) -> Self::Service {
        LogService {
            target: self.target,
            service,
        }
    }
}

pub struct LogService<S> {
    target: &'static str,
    service: S,
}

// impl<S, Request> Service<Request> for LogService<S>
// where
//     S: Service<Request>,
//     Request: std::fmt::Debug,
// {
//     type Response = S::Response;
//     type Error = S::Error;
//     type Future = S::Future;

//     fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
//         self.service.poll_ready(cx)
//     }

//     fn call(&mut self, request: Request) -> Self::Future {
//         println!("processing {:?}", request);
//         self.service.call(request)
//     }
// }

impl<S> Service<Request<Body>> for LogService<S>
where
    S: Service<Request<Body>> + Clone + Send,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        println!("processing {:?}", request.headers());

        // async move {
        //     // Can we await in here?
        //     let name = extract_rpc_method_name(request).await.unwrap();
        //     dbg!(&name);
        // };

        self.service.call(request)
    }
}

async fn extract_rpc_method_name(req: Request<Body>) -> Result<String, hyper::StatusCode> {
    if let Some(content_type) = req.headers().get("content-type") {
        if let Ok(content_type) = content_type.to_str() {
            if content_type != "application/json" {
                return Err(hyper::StatusCode::UNSUPPORTED_MEDIA_TYPE);
            }
        }
    } else {
        return Err(hyper::StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    let full_body = hyper::body::to_bytes(req.into_body()).await.unwrap();

    let json_body: Result<JsonRpcRequestObject, _> = serde_json::from_slice(&full_body);

    match json_body {
        Ok(json_rpc_request) => Ok(json_rpc_request.method_ref().into()),
        Err(_) => Err(hyper::StatusCode::BAD_REQUEST),
    }
}

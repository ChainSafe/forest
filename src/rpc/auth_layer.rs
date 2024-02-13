// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use http::Request;
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

impl<S, B> Service<Request<B>> for LogService<S>
where
    S: Service<Request<B>> + Clone + Send,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        println!("processing {:?}", request.headers());
        self.service.call(request)
    }
}

// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::auth::{verify_token, JWT_IDENTIFIER};
use crate::key_management::KeyStore;
use crate::rpc::AUTH_VERIFY;

use futures::future::BoxFuture;
use futures::FutureExt;
use hyper::StatusCode;
use jsonrpsee::server::middleware::rpc::{ResponseFuture, RpcServiceT};
use jsonrpsee::types::Id;
use jsonrpsee::types::Request as JsonRpcRequest;
use jsonrpsee::{MethodResponse, Methods};
use tokio::sync::RwLock;
use tower::Layer;
use tracing::info;

use std::sync::Arc;

#[derive(Clone)]
pub struct AuthLayer {
    pub headers: hyper::HeaderMap,
    pub keystore: Arc<RwLock<KeyStore>>,
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        AuthMiddleware {
            headers: self.headers.clone(),
            keystore: self.keystore.clone(),
            inner: service,
        }
    }
}

#[derive(Clone)]
pub struct AuthMiddleware<S> {
    pub headers: hyper::HeaderMap,
    pub inner: S,
    pub keystore: Arc<RwLock<KeyStore>>,
}

impl<'a, S> RpcServiceT<'a> for AuthMiddleware<S>
where
    S: Send + Clone + Sync + RpcServiceT<'a>,
{
    type Future = ResponseFuture<S::Future>;

    fn call(&self, req: jsonrpsee::types::Request<'a>) -> Self::Future {
        dbg!(&req.method_name());

        dbg!(&self.headers.get(hyper::header::AUTHORIZATION));

        let authorization_header = self.headers.get(hyper::header::AUTHORIZATION).cloned();

        if let Some(token) = authorization_header {
            let token = token.to_str().unwrap();

            // call auth_verify here with token

            ResponseFuture::future(self.inner.call(req))
        } else {
            // If no token is passed, assume read behavior

            // check ACCESS_MAP here, if ok return call(req) else return an error
            ResponseFuture::future(self.inner.call(req))
        }
    }
}

async fn auth_verify(token: &str, keystore: Arc<RwLock<KeyStore>>) -> anyhow::Result<Vec<String>> {
    let ks = keystore.read().await;
    let ki = ks.get(JWT_IDENTIFIER)?;
    let perms = verify_token(token, ki.private_key())?;
    Ok(perms)
}

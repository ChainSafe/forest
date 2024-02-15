// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::auth::{verify_token, JWT_IDENTIFIER};
use crate::key_management::KeyStore;
use crate::rpc_api::{check_access, ACCESS_MAP};

use futures::future::BoxFuture;
use futures::FutureExt;
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use jsonrpsee::types::{error::ErrorCode, ErrorObject, Id};
use jsonrpsee::MethodResponse;
use tokio::sync::RwLock;
use tower::Layer;
use tracing::debug;

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

// impl<'a, S> RpcServiceT<'a> for AuthMiddleware<S>
// where
//     S: Send + Clone + Sync + RpcServiceT<'a>,
// {
//     //type Future = ResponseFuture<S::Future>;
//     type Future = BoxFuture<'a, MethodResponse>;

//     fn call(&self, req: jsonrpsee::types::Request<'a>) -> Self::Future {
//         dbg!(&req.method_name());

//         dbg!(&self.headers.get(hyper::header::AUTHORIZATION));

//         let authorization_header = self.headers.get(hyper::header::AUTHORIZATION).cloned();

//         if let Some(token) = authorization_header {
//             let token = token.to_str().unwrap();

//             // call auth_verify here with token

//             ResponseFuture::future(self.inner.call(req))
//         } else {
//             // If no token is passed, assume read behavior

//             // check ACCESS_MAP here, if ok return call(req) else return an error
//             ResponseFuture::future(self.inner.call(req))
//         }
//     }
// }

impl<'a, S> RpcServiceT<'a> for AuthMiddleware<S>
where
    S: RpcServiceT<'a> + Send + Sync + Clone + 'static,
{
    type Future = BoxFuture<'a, MethodResponse>;

    fn call(&self, req: jsonrpsee::types::Request<'a>) -> Self::Future {
        dbg!(&req.method_name());

        dbg!(&self.headers.get(hyper::header::AUTHORIZATION));

        let service = self.inner.clone();
        let keystore = self.keystore.clone();
        let headers = self.headers.clone();

        async move {
            let auth_header = headers.get(hyper::header::AUTHORIZATION).cloned();
            let res = check_permissions(keystore, auth_header, req.method_name()).await;

            match res {
                Ok(()) => {
                    let resp = service.call(req).await;
                    resp
                }
                Err(code) => MethodResponse::error(Id::Null, ErrorObject::from(code)),
            }
        }
        .boxed()
    }
}

async fn auth_verify(token: &str, keystore: Arc<RwLock<KeyStore>>) -> anyhow::Result<Vec<String>> {
    let ks = keystore.read().await;
    let ki = ks.get(JWT_IDENTIFIER)?;
    let perms = verify_token(token, ki.private_key())?;
    Ok(perms)
}

async fn check_permissions(
    keystore: Arc<RwLock<KeyStore>>,
    auth_header: Option<hyper::header::HeaderValue>,
    method: &str,
) -> Result<(), ErrorCode> {
    let claims = match auth_header {
        Some(token) => {
            let token = token.to_str().map_err(|_| ErrorCode::InternalError)?;

            debug!("JWT from HTTP Header: {}", token);

            auth_verify(token, keystore)
                .await
                .map_err(|_| ErrorCode::InternalError)?
        }
        // If no token is passed, assume read behavior
        None => vec!["read".to_owned()],
    };
    debug!("Decoded JWT Claims: {}", claims.join(","));

    match ACCESS_MAP.get(&method) {
        Some(access) => {
            if check_access(access, &claims) {
                Ok(())
            } else {
                Err(ErrorCode::InvalidRequest)
            }
        }
        None => Err(ErrorCode::MethodNotFound),
    }
}

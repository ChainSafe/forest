// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::auth::{verify_token, JWT_IDENTIFIER};
use crate::key_management::KeyStore;
use crate::rpc::{chain, Permission, RpcMethod as _, CANCEL_METHOD_NAME};
use ahash::{HashMap, HashMapExt as _};
use futures::future::BoxFuture;
use futures::FutureExt;
use http::{
    header::{HeaderValue, AUTHORIZATION},
    HeaderMap,
};
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use jsonrpsee::types::{error::ErrorCode, ErrorObject};
use jsonrpsee::MethodResponse;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::Layer;
use tracing::debug;

static METHOD_NAME2REQUIRED_PERMISSION: Lazy<HashMap<&str, Permission>> = Lazy::new(|| {
    let mut access = HashMap::new();

    macro_rules! insert {
        ($ty:ty) => {
            access.insert(<$ty>::NAME, <$ty>::PERMISSION);

            if let Some(alias) = <$ty>::NAME_ALIAS {
                access.insert(alias, <$ty>::PERMISSION);
            }
        };
    }
    super::for_each_rpc_method!(insert);

    access.insert(chain::CHAIN_NOTIFY, Permission::Read);
    access.insert(CANCEL_METHOD_NAME, Permission::Read);

    access
});

fn is_allowed(required_by_method: Permission, claimed_by_user: &[String]) -> bool {
    let needle = match required_by_method {
        Permission::Admin => "admin",
        Permission::Sign => "sign",
        Permission::Write => "write",
        Permission::Read => "read",
    };
    claimed_by_user.iter().any(|haystack| haystack == needle)
}

#[derive(Clone)]
pub struct AuthLayer {
    pub headers: HeaderMap,
    pub keystore: Arc<RwLock<KeyStore>>,
}

impl<S> Layer<S> for AuthLayer {
    type Service = Auth<S>;

    fn layer(&self, service: S) -> Self::Service {
        Auth {
            headers: self.headers.clone(),
            keystore: self.keystore.clone(),
            service,
        }
    }
}

#[derive(Clone)]
pub struct Auth<S> {
    headers: HeaderMap,
    keystore: Arc<RwLock<KeyStore>>,
    service: S,
}

impl<'a, S> RpcServiceT<'a> for Auth<S>
where
    S: RpcServiceT<'a> + Send + Sync + Clone + 'static,
{
    type Future = BoxFuture<'a, MethodResponse>;

    fn call(&self, req: jsonrpsee::types::Request<'a>) -> Self::Future {
        let headers = self.headers.clone();
        let keystore = self.keystore.clone();
        let service = self.service.clone();

        async move {
            let auth_header = headers.get(AUTHORIZATION).cloned();
            let res = check_permissions(keystore, auth_header, req.method_name()).await;

            match res {
                Ok(true) => service.call(req).await,
                Ok(false) => MethodResponse::error(
                    req.id(),
                    ErrorObject::borrowed(
                        http::StatusCode::UNAUTHORIZED.as_u16() as _,
                        "Unauthorized",
                        None,
                    ),
                ),
                Err(code) => MethodResponse::error(req.id(), ErrorObject::from(code)),
            }
        }
        .boxed()
    }
}

/// Verify JWT Token and return the token's permissions.
async fn auth_verify(token: &str, keystore: Arc<RwLock<KeyStore>>) -> anyhow::Result<Vec<String>> {
    let ks = keystore.read().await;
    let ki = ks.get(JWT_IDENTIFIER)?;
    let perms = verify_token(token, ki.private_key())?;
    Ok(perms)
}

async fn check_permissions(
    keystore: Arc<RwLock<KeyStore>>,
    auth_header: Option<HeaderValue>,
    method: &str,
) -> anyhow::Result<bool, ErrorCode> {
    let claims = match auth_header {
        Some(token) => {
            let token = token
                .to_str()
                .map_err(|_| ErrorCode::ParseError)?
                .trim_start_matches("Bearer ");

            debug!("JWT from HTTP Header: {}", token);

            auth_verify(token, keystore)
                .await
                .map_err(|_| ErrorCode::InvalidRequest)?
        }
        // If no token is passed, assume read behavior
        None => vec!["read".to_owned()],
    };
    debug!("Decoded JWT Claims: {}", claims.join(","));

    match METHOD_NAME2REQUIRED_PERMISSION.get(&method) {
        Some(required_by_method) => Ok(is_allowed(*required_by_method, &claims)),
        None => Err(ErrorCode::MethodNotFound),
    }
}

#[cfg(test)]
mod tests {
    use self::chain::ChainHead;
    use super::*;
    use crate::rpc::wallet;
    use chrono::Duration;

    #[tokio::test]
    async fn check_permissions_no_header() {
        let keystore = Arc::new(RwLock::new(
            KeyStore::new(crate::KeyStoreConfig::Memory).unwrap(),
        ));

        let res = check_permissions(keystore.clone(), None, ChainHead::NAME).await;
        assert_eq!(res, Ok(true));

        let res = check_permissions(keystore.clone(), None, "Cthulhu.InvokeElderGods").await;
        assert_eq!(res.unwrap_err(), ErrorCode::MethodNotFound);

        let res = check_permissions(keystore.clone(), None, wallet::WalletNew::NAME).await;
        assert_eq!(res, Ok(false));
    }

    #[tokio::test]
    async fn check_permissions_invalid_header() {
        let keystore = Arc::new(RwLock::new(
            KeyStore::new(crate::KeyStoreConfig::Memory).unwrap(),
        ));

        let auth_header = HeaderValue::from_static("Bearer Azathoth");
        let res = check_permissions(keystore.clone(), Some(auth_header), ChainHead::NAME).await;
        assert_eq!(res.unwrap_err(), ErrorCode::InvalidRequest);

        let auth_header = HeaderValue::from_static("Cthulhu");
        let res = check_permissions(keystore.clone(), Some(auth_header), ChainHead::NAME).await;
        assert_eq!(res.unwrap_err(), ErrorCode::InvalidRequest);
    }

    #[tokio::test]
    async fn check_permissions_valid_header() {
        use crate::auth::*;
        let keystore = Arc::new(RwLock::new(
            KeyStore::new(crate::KeyStoreConfig::Memory).unwrap(),
        ));

        // generate a key and store it in the keystore
        let key_info = generate_priv_key();
        keystore
            .write()
            .await
            .put(JWT_IDENTIFIER, key_info.clone())
            .unwrap();
        let token_exp = Duration::hours(1);
        let token = create_token(
            ADMIN.iter().map(ToString::to_string).collect(),
            key_info.private_key(),
            token_exp,
        )
        .unwrap();

        // Should work with the `Bearer` prefix
        let auth_header = HeaderValue::from_str(&format!("Bearer {token}")).unwrap();
        let res =
            check_permissions(keystore.clone(), Some(auth_header.clone()), ChainHead::NAME).await;
        assert_eq!(res, Ok(true));

        let res = check_permissions(
            keystore.clone(),
            Some(auth_header.clone()),
            wallet::WalletNew::NAME,
        )
        .await;
        assert_eq!(res, Ok(true));

        // Should work without the `Bearer` prefix
        let auth_header = HeaderValue::from_str(&token).unwrap();
        let res =
            check_permissions(keystore.clone(), Some(auth_header), wallet::WalletNew::NAME).await;
        assert_eq!(res, Ok(true));
    }
}

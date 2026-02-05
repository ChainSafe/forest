// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::auth::{JWT_IDENTIFIER, verify_token};
use crate::key_management::KeyStore;
use crate::rpc::{CANCEL_METHOD_NAME, Permission, RpcMethod as _, chain};
use ahash::{HashMap, HashMapExt as _};
use futures::future::Either;
use http::{
    HeaderMap,
    header::{AUTHORIZATION, HeaderValue},
};
use itertools::Itertools as _;
use jsonrpsee::MethodResponse;
use jsonrpsee::core::middleware::{Batch, BatchEntry, BatchEntryErr, Notification};
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use jsonrpsee::types::Id;
use jsonrpsee::types::{ErrorObject, error::ErrorCode};
use parking_lot::RwLock;
use std::sync::{Arc, LazyLock};
use tower::Layer;
use tracing::debug;

static METHOD_NAME2REQUIRED_PERMISSION: LazyLock<HashMap<&str, Permission>> = LazyLock::new(|| {
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

impl<S> Auth<S> {
    fn authorize<'a>(&self, method_name: &str) -> Result<(), ErrorObject<'a>> {
        match check_permissions(&self.keystore, self.headers.get(AUTHORIZATION), method_name) {
            Ok(true) => Ok(()),
            Ok(false) => {
                tracing::warn!("Unauthorized access attempt for method {method_name}");
                Err(ErrorObject::borrowed(
                    http::StatusCode::UNAUTHORIZED.as_u16() as _,
                    "Unauthorized",
                    None,
                ))
            }
            Err(code) => {
                tracing::warn!("Authorization error for method {method_name}: {code:?}");
                Err(ErrorObject::from(code))
            }
        }
    }
}

impl<S> RpcServiceT for Auth<S>
where
    S: RpcServiceT<
            MethodResponse = MethodResponse,
            NotificationResponse = MethodResponse,
            BatchResponse = MethodResponse,
        > + Send
        + Sync
        + Clone
        + 'static,
{
    type MethodResponse = S::MethodResponse;
    type NotificationResponse = S::NotificationResponse;
    type BatchResponse = S::BatchResponse;

    fn call<'a>(
        &self,
        req: jsonrpsee::types::Request<'a>,
    ) -> impl Future<Output = Self::MethodResponse> + Send + 'a {
        match self.authorize(req.method_name()) {
            Ok(()) => Either::Left(self.service.call(req)),
            Err(e) => Either::Right(async move { MethodResponse::error(req.id(), e) }),
        }
    }

    fn notification<'a>(
        &self,
        n: Notification<'a>,
    ) -> impl Future<Output = Self::NotificationResponse> + Send + 'a {
        match self.authorize(n.method_name()) {
            Ok(()) => Either::Left(self.service.notification(n)),
            Err(e) => Either::Right(async move { MethodResponse::error(Id::Null, e) }),
        }
    }

    fn batch<'a>(&self, batch: Batch<'a>) -> impl Future<Output = Self::BatchResponse> + Send + 'a {
        let entries = batch
            .into_iter()
            .filter_map(|entry| match entry {
                Ok(BatchEntry::Call(req)) => Some(match self.authorize(req.method_name()) {
                    Ok(()) => Ok(BatchEntry::Call(req)),
                    Err(e) => Err(BatchEntryErr::new(req.id(), e)),
                }),
                Ok(BatchEntry::Notification(n)) => match self.authorize(n.method_name()) {
                    Ok(_) => Some(Ok(BatchEntry::Notification(n))),
                    Err(_) => None,
                },
                Err(err) => Some(Err(err)),
            })
            .collect_vec();
        self.service.batch(Batch::from(entries))
    }
}

/// Verify JWT Token and return the token's permissions.
fn auth_verify(token: &str, keystore: &RwLock<KeyStore>) -> anyhow::Result<Vec<String>> {
    let key_info = keystore.read().get(JWT_IDENTIFIER)?;
    Ok(verify_token(token, key_info.private_key())?)
}

fn check_permissions(
    keystore: &RwLock<KeyStore>,
    auth_header: Option<&HeaderValue>,
    method: &str,
) -> Result<bool, ErrorCode> {
    let claims = match auth_header {
        Some(token) => {
            let token = token
                .to_str()
                .map_err(|_| ErrorCode::ParseError)?
                .trim_start_matches("Bearer ");

            debug!("JWT from HTTP Header: {}", token);

            auth_verify(token, keystore).map_err(|_| ErrorCode::InvalidRequest)?
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

    #[test]
    fn check_permissions_no_header() {
        let keystore = Arc::new(RwLock::new(
            KeyStore::new(crate::KeyStoreConfig::Memory).unwrap(),
        ));

        let res = check_permissions(&keystore, None, ChainHead::NAME);
        assert_eq!(res, Ok(true));

        let res = check_permissions(&keystore, None, "Cthulhu.InvokeElderGods");
        assert_eq!(res.unwrap_err(), ErrorCode::MethodNotFound);

        let res = check_permissions(&keystore, None, wallet::WalletNew::NAME);
        assert_eq!(res, Ok(false));
    }

    #[test]
    fn check_permissions_invalid_header() {
        let keystore = Arc::new(RwLock::new(
            KeyStore::new(crate::KeyStoreConfig::Memory).unwrap(),
        ));

        let auth_header = HeaderValue::from_static("Bearer Azathoth");
        let res = check_permissions(&keystore, Some(&auth_header), ChainHead::NAME);
        assert_eq!(res.unwrap_err(), ErrorCode::InvalidRequest);

        let auth_header = HeaderValue::from_static("Cthulhu");
        let res = check_permissions(&keystore, Some(&auth_header), ChainHead::NAME);
        assert_eq!(res.unwrap_err(), ErrorCode::InvalidRequest);
    }

    #[test]
    fn check_permissions_valid_header() {
        use crate::auth::*;
        let keystore = Arc::new(RwLock::new(
            KeyStore::new(crate::KeyStoreConfig::Memory).unwrap(),
        ));

        // generate a key and store it in the keystore
        let key_info = generate_priv_key();
        keystore
            .write()
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
        let res = check_permissions(&keystore, Some(&auth_header), ChainHead::NAME);
        assert_eq!(res, Ok(true));

        let res = check_permissions(&keystore, Some(&auth_header), wallet::WalletNew::NAME);
        assert_eq!(res, Ok(true));

        // Should work without the `Bearer` prefix
        let auth_header = HeaderValue::from_str(&token).unwrap();
        let res = check_permissions(&keystore, Some(&auth_header), wallet::WalletNew::NAME);
        assert_eq!(res, Ok(true));
    }
}

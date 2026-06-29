// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::auth::{JWT_IDENTIFIER, verify_token};
use crate::key_management::KeyStore;
use crate::prelude::*;
use crate::rpc::{CANCEL_METHOD_NAME, Permission, RpcMethod as _, chain};
use ahash::HashMap;
use futures::future::Either;
use http::{
    HeaderMap,
    header::{AUTHORIZATION, HeaderValue},
};
use jsonrpsee::MethodResponse;
use jsonrpsee::core::middleware::{Batch, BatchEntry, BatchEntryErr, Notification};
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use jsonrpsee::types::Id;
use jsonrpsee::types::{ErrorObject, ErrorObjectOwned, error::ErrorCode};
use parking_lot::RwLock;
use std::sync::LazyLock;
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
    /// Permission claims resolved once for this connection. `Err` means the
    /// auth header was malformed or the token failed verification, in which
    /// case every call on this connection is rejected with that error.
    claims: Result<Arc<[String]>, ErrorObjectOwned>,
}

impl AuthLayer {
    pub fn new(headers: &HeaderMap, keystore: &RwLock<KeyStore>) -> Self {
        // Resolve the JWT claims once per connection (e.g. at the WebSocket
        // upgrade) instead of re-verifying the token on every request. This
        // matches Lotus, which authenticates once when the connection is
        // established. Note that a long-lived connection therefore keeps the
        // permissions it was granted at connection time; token expiry is not
        // re-checked mid-session.
        let claims = resolve_claims(keystore, headers.get(AUTHORIZATION)).map(Into::into);
        Self { claims }
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = Auth<S>;

    fn layer(&self, service: S) -> Self::Service {
        Auth {
            claims: self.claims.clone(),
            service,
        }
    }
}

#[derive(Clone)]
pub struct Auth<S> {
    /// Permission claims resolved once for this connection. `Err` means the
    /// auth header was malformed or the token failed verification, in which
    /// case every call on this connection is rejected with that error.
    claims: Result<Arc<[String]>, ErrorObjectOwned>,
    service: S,
}

impl<S> Auth<S> {
    fn authorize(&self, method_name: &str) -> Result<(), ErrorObjectOwned> {
        let claims = match &self.claims {
            Ok(claims) => claims,
            Err(err) => {
                // no need to spam the provider with this error; bad inputs are client-side issues
                tracing::debug!(
                    "Authorization error for method {method_name}: {}",
                    err.message()
                );
                return Err(err.clone());
            }
        };
        match is_method_allowed(claims, method_name) {
            Ok(true) => Ok(()),
            Ok(false) => {
                tracing::warn!("Unauthorized access attempt for method {method_name}");
                Err(unauthorized("insufficient permissions for method"))
            }
            Err(code) => Err(ErrorObject::from(code)),
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

/// Build an `Unauthorized` JSON-RPC error carrying a helpful, human-readable message. We use the HTTP `401 Unauthorized` status as the error code (rather than a JSON-RPC `-32600 Invalid request`) so that authentication failures are not confused with a malformed JSON-RPC request object. It's also what Lotus does so we match it for compatibility.
fn unauthorized(detail: &str) -> ErrorObjectOwned {
    ErrorObject::owned(
        i32::from(http::StatusCode::UNAUTHORIZED.as_u16()),
        format!("Unauthorized: {detail}"),
        None::<()>,
    )
}

/// Verify JWT Token and return the token's permissions.
fn auth_verify(token: &str, keystore: &RwLock<KeyStore>) -> anyhow::Result<Vec<String>> {
    let key_info = keystore.read().get(JWT_IDENTIFIER)?;
    Ok(verify_token(token, key_info.private_key())?)
}

/// Verify the auth header's JWT and return the token's permission claims.
///
/// This performs the (relatively expensive) JWT verification and is intended to
/// be called once per connection, not once per request.
fn resolve_claims(
    keystore: &RwLock<KeyStore>,
    auth_header: Option<&HeaderValue>,
) -> Result<Vec<String>, ErrorObjectOwned> {
    let claims = match auth_header {
        Some(header) => {
            let token = header
                .to_str()
                .map_err(|_| unauthorized("malformed authorization header"))?
                .strip_prefix("Bearer ")
                .ok_or_else(|| unauthorized("malformed authorization header"))?;

            auth_verify(token, keystore).map_err(|_| unauthorized("invalid authorization token"))?
        }
        // If no token is passed, assume read behavior
        None => vec!["read".to_owned()],
    };
    debug!("Decoded JWT Claims: {}", claims.join(","));
    Ok(claims)
}

/// Check whether the already-resolved `claims` permit calling `method`.
fn is_method_allowed(claims: &[String], method: &str) -> Result<bool, ErrorCode> {
    match METHOD_NAME2REQUIRED_PERMISSION.get(&method) {
        Some(required_by_method) => Ok(is_allowed(*required_by_method, claims)),
        None => Err(ErrorCode::MethodNotFound),
    }
}

/// Combined token resolution and permission check. Now that the connection path
/// resolves claims once via [`resolve_claims`], this is only used by tests.
#[cfg(test)]
fn check_permissions(
    keystore: &RwLock<KeyStore>,
    auth_header: Option<&HeaderValue>,
    method: &str,
) -> Result<bool, ErrorObjectOwned> {
    let claims = resolve_claims(keystore, auth_header)?;
    is_method_allowed(&claims, method).map_err(ErrorObject::from)
}

#[cfg(test)]
mod tests {
    use self::chain::ChainHead;
    use super::*;
    use crate::rpc::wallet;
    use chrono::Duration;

    /// Assert that `err` is a `401 Unauthorized` JSON-RPC error whose message
    /// mentions `detail`.
    #[track_caller]
    fn assert_unauthorized(err: &ErrorObjectOwned, detail: &str) {
        assert_eq!(
            err.code(),
            i32::from(http::StatusCode::UNAUTHORIZED.as_u16())
        );
        assert!(
            err.message().contains(detail),
            "unexpected message: {}",
            err.message()
        );
    }

    #[test]
    fn check_permissions_no_header() {
        let keystore = Arc::new(RwLock::new(
            KeyStore::new(crate::KeyStoreConfig::Memory).unwrap(),
        ));

        let res = check_permissions(&keystore, None, ChainHead::NAME);
        assert_eq!(res, Ok(true));

        let res = check_permissions(&keystore, None, "Cthulhu.InvokeElderGods");
        assert_eq!(res.unwrap_err().code(), ErrorCode::MethodNotFound.code());

        let res = check_permissions(&keystore, None, wallet::WalletNew::NAME);
        assert_eq!(res, Ok(false));
    }

    #[test]
    fn check_permissions_invalid_header() {
        let keystore = Arc::new(RwLock::new(
            KeyStore::new(crate::KeyStoreConfig::Memory).unwrap(),
        ));

        let auth_header = HeaderValue::from_static("Bearer Azathoth");
        let err = check_permissions(&keystore, Some(&auth_header), ChainHead::NAME).unwrap_err();
        assert_unauthorized(&err, "invalid authorization token");

        // No `Bearer ` scheme prefix: malformed, not merely an invalid token.
        let auth_header = HeaderValue::from_static("Cthulhu");
        let err = check_permissions(&keystore, Some(&auth_header), ChainHead::NAME).unwrap_err();
        assert_unauthorized(&err, "malformed authorization header");
    }

    #[test]
    fn check_permissions_non_utf8_header() {
        let keystore = Arc::new(RwLock::new(
            KeyStore::new(crate::KeyStoreConfig::Memory).unwrap(),
        ));

        // A header value that is not valid UTF-8 cannot be a JWT.
        let auth_header = HeaderValue::from_bytes(&[0xff, 0xfe]).unwrap();
        let err = check_permissions(&keystore, Some(&auth_header), ChainHead::NAME).unwrap_err();
        assert_unauthorized(&err, "malformed authorization header");
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

        // A header without the `Bearer ` scheme is malformed and rejected, even
        // if the bare value is itself a valid token.
        let auth_header = HeaderValue::from_str(&token).unwrap();
        let err =
            check_permissions(&keystore, Some(&auth_header), wallet::WalletNew::NAME).unwrap_err();
        assert_unauthorized(&err, "malformed authorization header");

        // Only a single `Bearer ` prefix is stripped: a doubled prefix leaves a
        // `Bearer ...` value that is not a valid token, so it is rejected.
        let auth_header = HeaderValue::from_str(&format!("Bearer Bearer {token}")).unwrap();
        let err =
            check_permissions(&keystore, Some(&auth_header), wallet::WalletNew::NAME).unwrap_err();
        assert_unauthorized(&err, "invalid authorization token");
    }

    /// `AuthLayer::layer` resolves the connection's token to claims exactly once
    /// (at connection setup); the resulting [`Auth`] service then authorizes
    /// calls against those cached claims without re-verifying the token.
    #[test]
    fn layer_resolves_claims_once_from_connection_header() {
        use crate::auth::*;
        let keystore = Arc::new(RwLock::new(
            KeyStore::new(crate::KeyStoreConfig::Memory).unwrap(),
        ));
        let key_info = generate_priv_key();
        keystore
            .write()
            .put(JWT_IDENTIFIER, key_info.clone())
            .unwrap();
        let token = create_token(
            ADMIN.iter().map(ToString::to_string).collect(),
            key_info.private_key(),
            Duration::hours(1),
        )
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );

        // Building the per-connection service resolves the claims once.
        let auth = AuthLayer::new(&headers, &keystore).layer(());
        let claims = auth.claims.clone().expect("admin token should resolve");
        assert!(claims.iter().any(|c| c == "admin"));

        // The cached claims authorize an admin method without touching the token again.
        assert!(auth.authorize(wallet::WalletNew::NAME).is_ok());
    }

    /// Cached claims are reused per request: a read-only connection is allowed
    /// read methods, rejected for write methods, and gets `MethodNotFound` for
    /// unknown methods.
    #[test]
    fn authorize_enforces_cached_permissions() {
        let auth = Auth {
            claims: Ok(vec!["read".to_owned()].into()),
            service: (),
        };

        assert!(auth.authorize(ChainHead::NAME).is_ok());

        let err = auth.authorize(wallet::WalletNew::NAME).unwrap_err();
        assert_eq!(
            err.code(),
            i32::from(http::StatusCode::UNAUTHORIZED.as_u16())
        );

        let err = auth.authorize("Cthulhu.InvokeElderGods").unwrap_err();
        assert_eq!(err.code(), ErrorCode::MethodNotFound.code());
    }

    /// A connection whose token failed verification caches the error and rejects
    /// every subsequent call with it, regardless of the method.
    #[test]
    fn authorize_with_failed_token_rejects_every_call() {
        let auth = Auth {
            claims: Err(unauthorized("invalid authorization token")),
            service: (),
        };

        let err = auth.authorize(ChainHead::NAME).unwrap_err();
        assert_unauthorized(&err, "invalid authorization token");

        let err = auth.authorize(wallet::WalletNew::NAME).unwrap_err();
        assert_unauthorized(&err, "invalid authorization token");
    }
}

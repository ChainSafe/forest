// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::auth::{JWT_IDENTIFIER, verify_token};
use crate::key_management::KeyStore;
use crate::prelude::*;
use crate::rpc::error::implementation_defined_errors::INSUFFICIENT_PERMISSIONS;
use crate::rpc::{CANCEL_METHOD_NAME, Permission, RpcMethod as _, chain};
use ahash::HashMap;
use futures::future::Either;
use http::header::HeaderValue;
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

/// The lowercase wire string for a permission, as it appears in a JWT `Allow`
/// claim list and in Lotus's permission errors.
fn permission_str(permission: Permission) -> &'static str {
    match permission {
        Permission::Admin => "admin",
        Permission::Sign => "sign",
        Permission::Write => "write",
        Permission::Read => "read",
    }
}

fn is_allowed(required_by_method: Permission, claimed_by_user: &[String]) -> bool {
    let needle = permission_str(required_by_method);
    claimed_by_user.iter().any(|haystack| haystack == needle)
}

#[derive(Clone)]
pub struct AuthLayer {
    /// Permission claims resolved once for this connection (via [`resolve_claims`]
    /// at the HTTP request / WebSocket upgrade). Token-verification failures are
    /// rejected with an HTTP `401` before this layer is built, so the claims are
    /// always present here.
    claims: Arc<[String]>,
}

impl AuthLayer {
    pub fn new(claims: Arc<[String]>) -> Self {
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
    claims: Arc<[String]>,
    service: S,
}

impl<S> Auth<S> {
    /// Authorize a single method call against this connection's permission claims.
    ///
    /// Returns a JSON-RPC error for an unknown method ([`ErrorCode::MethodNotFound`])
    /// or one the claims don't permit ([`INSUFFICIENT_PERMISSIONS`]). Token-level
    /// auth failures never reach here — they are rejected with an HTTP `401` at the
    /// transport layer before any JSON-RPC dispatch.
    fn authorize(&self, method_name: &str) -> Result<(), ErrorObjectOwned> {
        match METHOD_NAME2REQUIRED_PERMISSION.get(&method_name) {
            None => Err(ErrorObject::from(ErrorCode::MethodNotFound)),
            Some(&required) if is_allowed(required, &self.claims) => Ok(()),
            Some(&required) => {
                tracing::warn!("insufficient permissions to invoke method {method_name}");
                Err(insufficient_permissions(method_name, required))
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

/// Build the JSON-RPC error returned when an authenticated caller lacks the
/// permission a method requires. The message mirrors Lotus
/// (`missing permission to invoke '<method>' (need '<perm>')`), but the code is
/// Forest's implementation-defined [`INSUFFICIENT_PERMISSIONS`] rather than an
/// HTTP status masquerading as a JSON-RPC error code.
fn insufficient_permissions(method: &str, required: Permission) -> ErrorObjectOwned {
    ErrorObject::owned(
        INSUFFICIENT_PERMISSIONS,
        format!(
            "missing permission to invoke '{method}' (need '{}')",
            permission_str(required)
        ),
        None::<()>,
    )
}

/// Verify JWT Token and return the token's permissions.
fn auth_verify(token: &str, keystore: &RwLock<KeyStore>) -> anyhow::Result<Vec<String>> {
    let key_info = keystore.read().get(JWT_IDENTIFIER)?;
    Ok(verify_token(token, key_info.private_key())?)
}

/// Resolve the connection's `Authorization` header into permission claims.
///
/// This performs the (relatively expensive) JWT verification and is intended to
/// be called once per connection (at the HTTP request / WebSocket upgrade), not
/// once per request. Returns `Err(reason)` when the header is malformed or the
/// token fails verification; the caller rejects such connections with a bare
/// HTTP `401 Unauthorized` before any JSON-RPC dispatch, matching Lotus. The
/// `reason` is for server-side logging only and is not sent to the client.
pub(super) fn resolve_claims(
    keystore: &RwLock<KeyStore>,
    auth_header: Option<&HeaderValue>,
) -> Result<Arc<[String]>, &'static str> {
    let claims: Vec<String> = match auth_header {
        Some(header) => {
            let token = header
                .to_str()
                .map_err(|_| "malformed authorization header")?
                .strip_prefix("Bearer ")
                .ok_or("malformed authorization header")?;

            auth_verify(token, keystore).map_err(|_| "invalid authorization token")?
        }
        // If no token is passed, assume read behavior.
        None => vec!["read".to_owned()],
    };
    debug!("Decoded JWT permissions: {}", claims.join(","));
    Ok(claims.into())
}

#[cfg(test)]
mod tests {
    use self::chain::ChainHead;
    use super::*;
    use crate::rpc::wallet;
    use chrono::Duration;

    fn empty_keystore() -> Arc<RwLock<KeyStore>> {
        Arc::new(RwLock::new(
            KeyStore::new(crate::KeyStoreConfig::Memory).unwrap(),
        ))
    }

    /// Build a keystore with a JWT signing key and a matching `Bearer` token
    /// granting `perms`.
    fn keystore_with_token(perms: &[&str]) -> (Arc<RwLock<KeyStore>>, String) {
        use crate::auth::*;
        let keystore = empty_keystore();
        let key_info = generate_priv_key();
        keystore
            .write()
            .put(JWT_IDENTIFIER, key_info.clone())
            .unwrap();
        let token = create_token(
            perms.iter().map(ToString::to_string).collect(),
            key_info.private_key(),
            Duration::hours(1),
        )
        .unwrap();
        (keystore, token)
    }

    fn auth_with(claims: &[&str]) -> Auth<()> {
        Auth {
            claims: claims.iter().map(ToString::to_string).collect(),
            service: (),
        }
    }

    // --- resolve_claims: token-level outcomes (HTTP 401 is derived from `Err`) ---

    #[test]
    fn resolve_claims_no_header_defaults_to_read() {
        let claims = resolve_claims(&empty_keystore(), None).unwrap();
        assert_eq!(&*claims, &["read".to_owned()]);
    }

    #[test]
    fn resolve_claims_rejects_malformed_header() {
        let keystore = empty_keystore();

        // Not valid UTF-8, so it cannot be a JWT.
        let header = HeaderValue::from_bytes(&[0xff, 0xfe]).unwrap();
        assert_eq!(
            resolve_claims(&keystore, Some(&header)).unwrap_err(),
            "malformed authorization header"
        );

        // No `Bearer ` scheme prefix.
        let header = HeaderValue::from_static("Cthulhu");
        assert_eq!(
            resolve_claims(&keystore, Some(&header)).unwrap_err(),
            "malformed authorization header"
        );
    }

    #[test]
    fn resolve_claims_rejects_invalid_token() {
        let header = HeaderValue::from_static("Bearer Azathoth");
        assert_eq!(
            resolve_claims(&empty_keystore(), Some(&header)).unwrap_err(),
            "invalid authorization token"
        );
    }

    #[test]
    fn resolve_claims_accepts_valid_token() {
        let (keystore, token) = keystore_with_token(crate::auth::ADMIN);

        let header = HeaderValue::from_str(&format!("Bearer {token}")).unwrap();
        let claims = resolve_claims(&keystore, Some(&header)).unwrap();
        assert!(claims.iter().any(|c| c == "admin"));

        // A bare token without the `Bearer ` scheme is malformed, even though the
        // value itself is a valid token.
        let header = HeaderValue::from_str(&token).unwrap();
        assert_eq!(
            resolve_claims(&keystore, Some(&header)).unwrap_err(),
            "malformed authorization header"
        );

        // Only a single `Bearer ` prefix is stripped: a doubled prefix leaves a
        // `Bearer ...` value that is not a valid token.
        let header = HeaderValue::from_str(&format!("Bearer Bearer {token}")).unwrap();
        assert_eq!(
            resolve_claims(&keystore, Some(&header)).unwrap_err(),
            "invalid authorization token"
        );
    }

    // --- authorize: per-method permission checks against resolved claims ---

    #[test]
    fn authorize_allows_methods_within_permissions() {
        assert!(auth_with(&["read"]).authorize(ChainHead::NAME).is_ok());
    }

    #[test]
    fn authorize_denies_insufficient_permissions_with_jsonrpc_error() {
        let err = auth_with(&["read"])
            .authorize(wallet::WalletNew::NAME)
            .unwrap_err();
        // A JSON-RPC application error, NOT an HTTP status smuggled into the code.
        assert_eq!(err.code(), INSUFFICIENT_PERMISSIONS);
        assert_eq!(
            err.message(),
            format!(
                "missing permission to invoke '{}' (need 'write')",
                wallet::WalletNew::NAME
            )
        );
    }

    #[test]
    fn authorize_unknown_method_is_method_not_found() {
        let err = auth_with(&["read"])
            .authorize("Cthulhu.InvokeElderGods")
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::MethodNotFound.code());
    }

    /// Resolved admin claims, threaded through `AuthLayer::layer`, authorize a
    /// write method without re-touching the token.
    #[test]
    fn layer_propagates_resolved_claims() {
        let (keystore, token) = keystore_with_token(crate::auth::ADMIN);
        let header = HeaderValue::from_str(&format!("Bearer {token}")).unwrap();
        let claims = resolve_claims(&keystore, Some(&header)).unwrap();

        let auth = AuthLayer::new(claims).layer(());
        assert!(auth.authorize(wallet::WalletNew::NAME).is_ok());
    }
}

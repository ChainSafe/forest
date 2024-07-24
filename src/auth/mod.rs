// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::key_management::KeyInfo;
use crate::shim::crypto::SignatureType;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, errors::Result as JWTResult, DecodingKey, EncodingKey, Header};
use rand::Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// constant string that is used to identify the JWT secret key in `KeyStore`
pub const JWT_IDENTIFIER: &str = "auth-jwt-private";
/// Admin permissions
pub const ADMIN: &[&str] = &["read", "write", "sign", "admin"];
/// Signing permissions
pub const SIGN: &[&str] = &["read", "write", "sign"];
/// Writing permissions
pub const WRITE: &[&str] = &["read", "write"];
/// Reading permissions
pub const READ: &[&str] = &["read"];

/// Error enumeration for Authentication
#[derive(Debug, Error, Serialize, Deserialize)]
pub enum Error {
    /// Filecoin Method does not exist
    #[error("Filecoin method does not exist")]
    MethodParam,
    /// Invalid permissions to use specified method
    #[error("Incorrect permissions to access method")]
    InvalidPermissions,
    /// Missing authentication header
    #[error("Missing authentication header")]
    NoAuthHeader,
    #[error("{0}")]
    Other(String),
}

/// Claim structure for JWT Tokens
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    #[serde(rename = "Allow")]
    allow: Vec<String>,
    // Expiration time (as UTC timestamp)
    exp: Option<usize>,
}

/// Create a new JWT Token
pub fn create_token(
    perms: Vec<String>,
    key: &[u8],
    token_exp: Option<Duration>,
) -> JWTResult<String> {
    let exp = token_exp.map(|exp| (Utc::now() + exp).timestamp() as usize);
    let payload = Claims { allow: perms, exp };
    encode(&Header::default(), &payload, &EncodingKey::from_secret(key))
}

/// Verify JWT Token and return the allowed permissions from token
pub fn verify_token(token: &str, key: &[u8]) -> JWTResult<Vec<String>> {
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::default());
    validation.required_spec_claims = Default::default();

    let decoded = decode::<Claims>(token, &DecodingKey::from_secret(key), &validation)?;
    // Special case for admin permissions so that an SP isn't locked out of admin permissions.
    if !decoded.claims.allow.contains(&"admin".to_string()) {
        let validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::default());
        decode::<Claims>(token, &DecodingKey::from_secret(key), &validation)?;
    }

    Ok(decoded.claims.allow)
}

pub fn generate_priv_key() -> KeyInfo {
    let priv_key = rand::thread_rng().gen::<[u8; 32]>();
    // This is temporary use of bls key as placeholder, need to update keyinfo to use string
    // instead of keyinfo for key type
    KeyInfo::new(SignatureType::Bls, priv_key.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_verify_token() {
        let perms_expected = vec![
            "Ph'nglui mglw'nafh Cthulhu".to_owned(),
            "R'lyeh wgah'nagl fhtagn".to_owned(),
        ];
        let key = generate_priv_key();

        // Token duration of 1 hour. Validation must pass.
        let token = create_token(
            perms_expected.clone(),
            key.private_key(),
            Some(Duration::try_hours(1).expect("Infallible")),
        )
        .unwrap();
        let perms = verify_token(&token, key.private_key()).unwrap();
        assert_eq!(perms_expected, perms);

        // Token duration of -1 hour (already expired). Validation must fail.
        let token = create_token(
            perms_expected.clone(),
            key.private_key(),
            Some(-Duration::try_hours(1).expect("Infallible")),
        )
        .unwrap();
        assert!(verify_token(&token, key.private_key()).is_err());

        // Token duration of -10 seconds (already expired, slightly). There is leeway of 60 seconds
        // by default, so validation must pass.
        let token = create_token(
            perms_expected.clone(),
            key.private_key(),
            Some(-Duration::try_seconds(10).expect("Infallible")),
        )
        .unwrap();
        let perms = verify_token(&token, key.private_key()).unwrap();
        assert_eq!(perms_expected, perms);
    }
}

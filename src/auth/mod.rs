// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::key_management::KeyInfo;
use crate::shim::crypto::SignatureType;
use crate::utils::misc::env::is_env_truthy;
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, decode, encode, errors::Result as JWTResult};
use rand::Rng;
use serde::{Deserialize, Serialize};

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

/// Claim structure for JWT Tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Claims {
    #[serde(rename = "Allow")]
    allow: Vec<String>,
    // Expiration time (as UTC timestamp)
    #[serde(default)]
    exp: Option<usize>,
}

/// Create a new JWT Token
pub fn create_token(perms: Vec<String>, key: &[u8], token_exp: Duration) -> JWTResult<String> {
    let exp_time = Utc::now() + token_exp;
    let payload = Claims {
        allow: perms,
        exp: Some(exp_time.timestamp() as usize),
    };
    encode(&Header::default(), &payload, &EncodingKey::from_secret(key))
}

/// Verify JWT Token and return the allowed permissions from token
pub fn verify_token(token: &str, key: &[u8]) -> JWTResult<Vec<String>> {
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::default());
    if is_env_truthy("FOREST_JWT_DISABLE_EXP_VALIDATION") {
        let mut claims = validation.required_spec_claims.clone();
        claims.remove("exp");
        let buff: Vec<_> = claims.iter().collect();
        validation.set_required_spec_claims(&buff);
        validation.validate_exp = false;
    }
    let token = decode::<Claims>(token, &DecodingKey::from_secret(key), &validation)?;
    Ok(token.claims.allow)
}

pub fn generate_priv_key() -> KeyInfo {
    let priv_key = crate::utils::rand::forest_os_rng().r#gen::<[u8; 32]>();
    // This is temporary use of bls key as placeholder, need to update keyinfo to use string
    // instead of keyinfo for key type
    KeyInfo::new(SignatureType::Bls, priv_key.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a new JWT Token without expiration
    fn create_token_without_exp(perms: Vec<String>, key: &[u8]) -> JWTResult<String> {
        let payload = Claims {
            allow: perms,
            exp: None,
        };
        encode(&Header::default(), &payload, &EncodingKey::from_secret(key))
    }

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
            Duration::try_hours(1).expect("Infallible"),
        )
        .unwrap();
        let perms = verify_token(&token, key.private_key()).unwrap();
        assert_eq!(perms_expected, perms);

        // Token duration of -1 hour (already expired). Validation must fail.
        let token = create_token(
            perms_expected.clone(),
            key.private_key(),
            -Duration::try_hours(1).expect("Infallible"),
        )
        .unwrap();
        assert!(verify_token(&token, key.private_key()).is_err());

        // Token duration of -10 seconds (already expired, slightly). There is leeway of 60 seconds
        // by default, so validation must pass.
        let token = create_token(
            perms_expected.clone(),
            key.private_key(),
            -Duration::try_seconds(10).expect("Infallible"),
        )
        .unwrap();
        let perms = verify_token(&token, key.private_key()).unwrap();
        assert_eq!(perms_expected, perms);
    }

    #[test]
    fn create_and_verify_token_without_exp() {
        let perms_expected = vec![
            "Ia! Ia! Cthulhu fhtagn".to_owned(),
            "Zin-Mi-Yak, dread lord of the deep".to_owned(),
        ];
        let key = generate_priv_key();

        // Disable expiration validation via env var
        unsafe {
            std::env::set_var("FOREST_JWT_DISABLE_EXP_VALIDATION", "1");
        }

        // No exp at all in the token. Validation must pass.
        let token = create_token_without_exp(perms_expected.clone(), key.private_key()).unwrap();
        let perms = verify_token(&token, key.private_key()).unwrap();
        assert_eq!(perms_expected, perms);

        // Token duration of -1 hour (already expired). Validation must pass.
        let token = create_token(
            perms_expected.clone(),
            key.private_key(),
            -Duration::try_hours(1).expect("Infallible"),
        )
        .unwrap();
        let perms = verify_token(&token, key.private_key()).unwrap();
        assert_eq!(perms_expected, perms);
    }
}

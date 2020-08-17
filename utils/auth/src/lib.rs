// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

use crypto::SignatureType;
use jsonrpc_v2::Error as JsonRpcError;
use jsonwebtoken::errors::Result as JWTResult;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use wallet::KeyInfo;

/// Error Enum for Authentification
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

lazy_static! {
    /// Constants of all Levels of permissions
    pub static ref ADMIN: Vec<String> = vec![
        "read".to_string(),
        "write".to_string(),
        "sign".to_string(),
        "admin".to_string()
    ];
    pub static ref SIGN: Vec<String> =
        vec!["read".to_string(), "write".to_string(), "sign".to_string()];
    pub static ref WRITE: Vec<String> = vec!["read".to_string(), "write".to_string()];
    pub static ref READ: Vec<String> = vec!["read".to_string()];
}

/// Claim struct for JWT Tokens
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    #[serde(rename = "Allow")]
    allow: Vec<String>,
}

/// Create a new JWT Token
pub fn create_token(perms: Vec<String>, key: &[u8]) -> JWTResult<String> {
    let payload = Claims {
        allow: perms,
    };
    encode(&Header::default(), &payload, &EncodingKey::from_secret(key))
}

/// Verify JWT Token and return the allowed permissions from token
pub fn verify_token(token: &str, key: &[u8]) -> JWTResult<Vec<String>> {
    let mut validation = Validation::default();
    validation.validate_exp = false;
    let token = decode::<Claims>(
        token,
        &DecodingKey::from_secret(key),
        &validation,
    )?;
    Ok(token.claims.allow)
}

/// Check whether or not header has required permissions
pub fn has_perms(header_raw: String, required: &str, key: &[u8]) -> Result<(), JsonRpcError> {
    if header_raw.starts_with("Bearer: ") {
        let token = header_raw.trim_start_matches("Bearer: ");
        let perms = verify_token(token, key).map_err(|err| Error::Other(err.to_string()))?;
        if !perms.contains(&required.to_string()) {
            return Err(JsonRpcError::from(Error::InvalidPermissions));
        }
    }
    Ok(())
}

pub fn generate_priv_key() -> KeyInfo {
    let priv_key = rand::thread_rng().gen::<[u8; 32]>();
    // TODO temp use of bls key as placeholder, need to update keyinfo to use string instead of keyinfo
    // for key type
    KeyInfo::new(SignatureType::BLS, priv_key.to_vec())
}

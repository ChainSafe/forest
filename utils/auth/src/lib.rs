// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

use jsonrpc_v2::Error as JsonRpcError;
use jsonwebtoken::errors::Result as JWTResult;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use thiserror::Error;

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

// TODO need to figure out how lotus generates secret key for encoding and decoding JWT Tokens

/// Claim struct for JWT Tokens
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    #[serde(rename = "Allow")]
    allow: Vec<String>,
    // TODO currently lotus does not have an exp value for their JWT tokens, need to figure out what they do instead to overcome invalid validations
    exp: usize,
}

/// Create a new JWT Token
pub fn create_token(perms: Vec<String>) -> JWTResult<String> {
    let payload = Claims {
        allow: perms,
        /// TODO change value to proper expiration
        exp: 10000000000,
    };
    encode(
        &Header::default(),
        &payload,
        &EncodingKey::from_secret("secret".as_ref()),
    )
}

/// Verify JWT Token and return the allowed permissions from token
pub fn verify_token(token: &str) -> JWTResult<Vec<String>> {
    let token = decode::<Claims>(
        token,
        &DecodingKey::from_secret("secret".as_ref()),
        &Validation::default(),
    )?;
    Ok(token.claims.allow)
}

/// Check whether or not header has required permissions
pub fn has_perms(header_raw: String, required: &str) -> Result<(), JsonRpcError> {
    if header_raw.starts_with("Bearer: ") {
        let token = header_raw.trim_start_matches("Bearer: ");
        let perms = verify_token(token).map_err(|err| Error::Other(err.to_string()))?;
        if !perms.contains(&required.to_string()) {
            return Err(JsonRpcError::from(Error::InvalidPermissions));
        }
    }
    Ok(())
}

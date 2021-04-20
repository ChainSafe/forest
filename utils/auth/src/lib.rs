// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crypto::SignatureType;
use jsonrpc_v2::Error as JsonRpcError;
use jsonwebtoken::errors::Result as JWTResult;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header};
use rand::Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use wallet::KeyInfo;

/// constant string that is used to identify the JWT secret key in KeyStore
pub const JWT_IDENTIFIER: &str = "auth-jwt-private";
/// Admin permissions
pub const ADMIN: [&str; 4] = ["read", "write", "sign", "admin"];
/// Signing permissions
pub const SIGN: [&str; 3] = ["read", "write", "sign"];
/// Writing permissions
pub const WRITE: [&str; 2] = ["read", "write"];
/// Reading permissions
pub const READ: [&str; 1] = ["read"];
/// All methods that require write permission
pub const WRITE_ACCESS: [&str; 6] = [
    "Filecoin.MpoolPush",
    "Filecoin.WalletNew",
    "Filecoin.WalletHas",
    "Filecoin.WalletList",
    "Filecoin.WalletDefaultAddress",
    "Filecoin.WalletList",
];

/// Error Enum for Authentication
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

/// Claim struct for JWT Tokens
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    #[serde(rename = "Allow")]
    allow: Vec<String>,
}

/// Create a new JWT Token
pub fn create_token(perms: Vec<String>, key: &[u8]) -> JWTResult<String> {
    let payload = Claims { allow: perms };
    encode(&Header::default(), &payload, &EncodingKey::from_secret(key))
}

/// Verify JWT Token and return the allowed permissions from token
pub fn verify_token(token: &str, key: &[u8]) -> JWTResult<Vec<String>> {
    let validation = jsonwebtoken::Validation {
        validate_exp: false,
        ..Default::default()
    };
    let token = decode::<Claims>(token, &DecodingKey::from_secret(key), &validation)?;
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

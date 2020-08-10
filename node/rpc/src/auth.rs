// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpc_v2::{Error as JsonRpcError, Params};
use jsonwebtoken::errors::Result as JWTResult;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

lazy_static! {
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

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    // each string is a permission
    #[serde(rename = "Allow")]
    allow: Vec<String>,
    exp: usize,
}

/// Create a new JWT Token
pub fn create_token(perms: Vec<String>) -> JWTResult<String> {
    let payload = Claims {
        allow: perms,
        exp: 10000000000,
    };
    encode(
        &Header::default(),
        &payload,
        &EncodingKey::from_secret("secret".as_ref()),
    )
}

/// Verify JWT Token and return the allowed permissions from token
pub fn verify_token(token: String) -> JWTResult<Vec<String>> {
    let token = decode::<Claims>(
        &token,
        &DecodingKey::from_secret("secret".as_ref()),
        &Validation::default(),
    )?;
    Ok(token.claims.allow)
}

/// RPC call to create a new JWT Token
pub(crate) async fn auth_new(
    Params(params): Params<(Vec<String>,)>,
) -> Result<String, JsonRpcError> {
    let (perms,) = params;
    let token = create_token(perms)?;
    Ok(token)
}

/// RPC call to verify JWT Token and return the token's permissions
pub(crate) async fn auth_verify(
    Params(params): Params<(String,)>,
) -> Result<Vec<String>, JsonRpcError> {
    let (token,) = params;
    let perms = verify_token(token)?;
    Ok(perms)
}

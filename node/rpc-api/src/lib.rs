// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Access levels to be checked against JWT claims
pub enum Access {
    Admin,
    Sign,
    Write,
    Read,
}

/// Access mapping between method names and access levels
/// Checked against JWT claims on every request
pub static ACCESS_MAP: Lazy<HashMap<&str, Access>> = Lazy::new(|| {
    let mut access = HashMap::new();

    access.insert(auth_new::AUTH_NEW, Access::Admin);
    access.insert(auth_verify::AUTH_VERIFY, Access::Read);

    access
});

/// Checks an access enum against provided JWT claims
pub fn check_access(access: &Access, claims: &Vec<String>) -> bool {
    match access {
        Access::Admin => claims.contains(&"admin".to_owned()),
        Access::Sign => claims.contains(&"sign".to_owned()),
        Access::Write => claims.contains(&"write".to_owned()),
        Access::Read => claims.contains(&"read".to_owned()),
    }
}

/// JSON-RPC API definitions

/// Auth
pub mod auth_new {
    pub const AUTH_NEW: &str = "Filecoin.AuthNew";
    pub type AuthNewParams = (Vec<String>,);
    pub type AuthNewResult = Vec<u8>;
}

pub mod auth_verify {
    pub const AUTH_VERIFY: &str = "Filecoin.AuthVerify";
    pub type AuthVerifyParams = (String,);
    pub type AuthVerifyResult = Vec<String>;
}

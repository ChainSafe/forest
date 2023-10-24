// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::auth_api::*;
use chrono::Duration;

use super::{ApiInfo, JsonRpcError, RpcRequest};

impl ApiInfo {
    /// Creates a new JWT Token
    pub async fn auth_new(
        &self,
        perms: Vec<String>,
        token_exp: Duration,
    ) -> Result<AuthNewResult, JsonRpcError> {
        self.call(Self::auth_new_req(perms, token_exp)).await
    }

    pub fn auth_new_req(perms: Vec<String>, token_exp: Duration) -> RpcRequest<Vec<u8>> {
        RpcRequest::new(AUTH_NEW, AuthNewParams { perms, token_exp })
    }
}

// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod chain_ops;

use crate::lotus_json::HasLotusJson;
use crate::rpc::ApiVersion;
use jsonrpsee::core::traits::ToRpcParams;
use once_cell::sync::Lazy;
use std::{env, marker::PhantomData, time::Duration};

pub const API_INFO_KEY: &str = "FULLNODE_API_INFO";
pub const DEFAULT_PORT: u16 = 2345;

/// Default timeout for RPC requests. Doesn't apply to all requests, e.g., snapshot export which
/// has no timeout.
pub static DEFAULT_TIMEOUT: Lazy<Duration> = Lazy::new(|| {
    env::var("FOREST_RPC_DEFAULT_TIMEOUT")
        .ok()
        .and_then(|it| Duration::from_secs(it.parse().ok()?).into())
        .unwrap_or(Duration::from_secs(60))
});

/// An `RpcRequest` is an at-rest description of a remote procedure call. It can
/// be invoked using `ApiInfo::call`.
///
/// When adding support for a new RPC method, the corresponding `RpcRequest`
/// value should be public for use in testing.
#[derive(Debug, Clone)]
pub struct RpcRequest<T = serde_json::Value> {
    pub method_name: &'static str,
    pub params: serde_json::Value,
    pub result_type: PhantomData<T>,
    pub api_version: ApiVersion,
    pub timeout: Duration,
}

impl<T> RpcRequest<T> {
    pub fn new<P: HasLotusJson>(method_name: &'static str, params: P) -> Self {
        RpcRequest {
            method_name,
            params: params
                .into_lotus_json_value()
                .unwrap_or(serde_json::Value::String(
                    "INTERNAL ERROR: Parameters could not be serialized as JSON".to_string(),
                )),
            result_type: PhantomData,
            api_version: ApiVersion::V0,
            timeout: *DEFAULT_TIMEOUT,
        }
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.set_timeout(timeout);
        self
    }

    /// Map type information about the response.
    pub fn map_ty<U>(self) -> RpcRequest<U> {
        RpcRequest {
            method_name: self.method_name,
            params: self.params,
            result_type: PhantomData,
            api_version: self.api_version,
            timeout: self.timeout,
        }
    }
}

impl<T> ToRpcParams for RpcRequest<T> {
    fn to_rpc_params(self) -> Result<Option<Box<serde_json::value::RawValue>>, serde_json::Error> {
        Ok(Some(serde_json::value::to_raw_value(&self.params)?))
    }
}

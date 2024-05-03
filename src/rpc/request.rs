// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{chain::CHAIN_NOTIFY, ApiVersion};
use jsonrpsee::core::traits::ToRpcParams;
use std::{marker::PhantomData, time::Duration};

/// An at-rest description of a remote procedure call, created using
/// [`rpc::RpcMethodExt`](crate::rpc::RpcMethodExt::request), and called using [`rpc::Client::call`](crate::rpc::Client::call).
#[derive(Debug, Clone)]
pub struct Request<T = serde_json::Value> {
    pub method_name: &'static str,
    pub params: serde_json::Value,
    pub result_type: PhantomData<T>,
    pub api_version: ApiVersion,
    pub timeout: Duration,
}

impl<T> Request<T> {
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.set_timeout(timeout);
        self
    }

    /// Map type information about the response.
    pub fn map_ty<U>(self) -> Request<U> {
        Request {
            method_name: self.method_name,
            params: self.params,
            result_type: PhantomData,
            api_version: self.api_version,
            timeout: self.timeout,
        }
    }

    pub fn is_subscription_method(&self) -> bool {
        matches!(self.method_name, CHAIN_NOTIFY)
    }
}

impl<T> ToRpcParams for Request<T> {
    fn to_rpc_params(self) -> Result<Option<Box<serde_json::value::RawValue>>, serde_json::Error> {
        Ok(Some(serde_json::value::to_raw_value(&self.params)?))
    }
}

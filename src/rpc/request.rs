// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::ApiPaths;
use enumflags2::BitFlags;
use jsonrpsee::core::traits::ToRpcParams;
use serde::{Deserialize, Serialize};
use std::{marker::PhantomData, time::Duration};

/// An at-rest description of a remote procedure call, created using
/// [`rpc::RpcMethodExt`](crate::rpc::RpcMethodExt::request), and called using [`rpc::Client::call`](crate::rpc::Client::call).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request<T = serde_json::Value> {
    pub method_name: std::borrow::Cow<'static, str>,
    pub params: serde_json::Value,
    #[serde(skip)]
    pub result_type: PhantomData<T>,
    #[serde(skip)]
    pub api_paths: BitFlags<ApiPaths>,
    #[serde(skip)]
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
            api_paths: self.api_paths,
            timeout: self.timeout,
        }
    }
}

impl<T> ToRpcParams for Request<T> {
    fn to_rpc_params(self) -> Result<Option<Box<serde_json::value::RawValue>>, serde_json::Error> {
        Ok(Some(serde_json::value::to_raw_value(&self.params)?))
    }
}

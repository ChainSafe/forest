// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::ApiPaths;
use anyhow::Context as _;
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
    pub api_path: ApiPaths,
    #[serde(skip)]
    pub timeout: Duration,
}

impl<T> Request<T> {
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Sets the request timeout and returns the modified request.
    ///
    /// # Examples
    ///
    /// ```
    /// let req = Request::default().with_timeout(std::time::Duration::from_secs(5));
    /// ```
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.set_timeout(timeout);
        self
    }

    /// Set the API path used to route this request.
    ///
    /// This updates the request's internal `api_path` field, which controls routing/version selection
    /// for the RPC and is not included in serialized output.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use std::borrow::Cow;
    /// use serde_json::json;
    /// use std::marker::PhantomData;
    /// use crate::rpc::request::Request;
    /// use crate::rpc::ApiPaths;
    ///
    /// let mut req = Request {
    ///     method_name: Cow::Borrowed("test_method"),
    ///     params: json!({}),
    ///     result_type: PhantomData,
    ///     api_path: ApiPaths::V1,
    ///     timeout: Duration::from_secs(30),
    /// };
    ///
    /// req.set_api_path(ApiPaths::V2);
    /// assert_eq!(req.api_path, ApiPaths::V2);
    /// ```
    pub fn set_api_path(&mut self, api_path: ApiPaths) {
        self.api_path = api_path;
    }

    /// Sets the request's API path and returns the modified request for builder-style chaining.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::borrow::Cow;
    /// use std::marker::PhantomData;
    /// use std::time::Duration;
    /// // assume ApiPaths and Request are in scope
    ///
    /// let req = Request::<serde_json::Value> {
    ///     method_name: Cow::Borrowed("echo"),
    ///     params: serde_json::json!(null),
    ///     result_type: PhantomData,
    ///     api_path: ApiPaths::V1,
    ///     timeout: Duration::from_secs(30),
    /// };
    ///
    /// let req2 = req.with_api_path(ApiPaths::V2);
    /// assert_eq!(req2.api_path, ApiPaths::V2);
    /// ```
    pub fn with_api_path(mut self, api_path: ApiPaths) -> Self {
        self.set_api_path(api_path);
        self
    }

    /// Produce a new Request with the same method, params, api path, and timeout but with the response type changed to `U`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use std::borrow::Cow;
    /// use std::marker::PhantomData;
    ///
    /// // Construct a Request typed to return `serde_json::Value`.
    /// let req_value = Request {
    ///     method_name: Cow::Borrowed("example.method"),
    ///     params: serde_json::json!({}),
    ///     result_type: PhantomData::<serde_json::Value>,
    ///     api_path: ApiPaths::V1,
    ///     timeout: Duration::from_secs(30),
    /// };
    ///
    /// // Map the request to expect a `String` result instead.
    /// let req_string: Request<String> = req_value.map_ty();
    /// ```
    pub fn map_ty<U>(self) -> Request<U> {
    pub fn map_ty<U>(self) -> Request<U> {
        Request {
            method_name: self.method_name,
            params: self.params,
            result_type: PhantomData,
            api_path: self.api_path,
            timeout: self.timeout,
        }
    }

    /// Selects the highest API version present in the provided set of API-path flags.
    ///
    /// Returns the largest `ApiPaths` variant contained in `api_paths`, or an error if `api_paths` has
    /// no flags set.
    ///
    /// # Examples
    ///
    /// ```
    /// // Given a BitFlags<ApiPaths> value named `flags`, obtain the highest supported path:
    /// // let result = max_api_path(flags)?;
    /// // match result {
    /// //     ApiPaths::V3 => println!("use v3"),
    /// //     ApiPaths::V2 => println!("use v2"),
    /// //     ApiPaths::V1 => println!("use v1"),
    /// // }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn max_api_path(api_paths: BitFlags<ApiPaths>) -> anyhow::Result<ApiPaths> {
        api_paths.iter().max().context("No supported versions")
    }
}

impl<T> ToRpcParams for Request<T> {
    fn to_rpc_params(self) -> Result<Option<Box<serde_json::value::RawValue>>, serde_json::Error> {
        Ok(Some(serde_json::value::to_raw_value(&self.params)?))
    }
}

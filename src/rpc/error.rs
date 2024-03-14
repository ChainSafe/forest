// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::Display;

use jsonrpsee::types::error::{
    ErrorObjectOwned, INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE, PARSE_ERROR_CODE,
};

#[derive(derive_more::From, derive_more::Into, Debug)]
pub struct JsonRpcError {
    inner: ErrorObjectOwned,
}

impl JsonRpcError {
    fn new(code: i32, message: impl Display, data: impl Into<Option<serde_json::Value>>) -> Self {
        Self {
            inner: ErrorObjectOwned::owned(code, message.to_string(), data.into()),
        }
    }
    pub fn parse_error(message: impl Display, data: impl Into<Option<serde_json::Value>>) -> Self {
        Self::new(PARSE_ERROR_CODE, message, data)
    }
    pub fn internal_error(
        message: impl Display,
        data: impl Into<Option<serde_json::Value>>,
    ) -> Self {
        Self::new(INTERNAL_ERROR_CODE, message, data)
    }
    pub fn invalid_params(
        message: impl Display,
        data: impl Into<Option<serde_json::Value>>,
    ) -> Self {
        Self::new(INVALID_PARAMS_CODE, message, data)
    }
}

macro_rules! from2internal {
    ($($ty:ty),* $(,)?) => {
        $(
            impl From<$ty> for JsonRpcError {
                fn from(it: $ty) -> Self {
                    Self::internal_error(it, None)
                }
            }
        )*
    };
}

// TODO(forest): https://github.com/ChainSafe/forest/issues/3965
//               Just mapping everything to an internal error is not appropriate
from2internal! {
    anyhow::Error,
    base64::DecodeError,
    cid::multibase::Error,
    crate::chain::store::Error,
    crate::key_management::Error,
    crate::libp2p::ParseError,
    crate::message_pool::Error,
    crate::state_manager::Error,
    fil_actors_shared::fvm_ipld_amt::Error,
    futures::channel::oneshot::Canceled,
    fvm_ipld_encoding::Error,
    fvm_shared4::address::Error,
    jsonwebtoken::errors::Error,
    std::io::Error,
    std::time::SystemTimeError,
    tokio::task::JoinError,
}

impl<T> From<flume::SendError<T>> for JsonRpcError {
    fn from(e: flume::SendError<T>) -> Self {
        Self::internal_error(e, None)
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for JsonRpcError {
    fn from(e: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Self::internal_error(e, None)
    }
}

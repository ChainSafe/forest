// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::{self, Display};

use jsonrpsee::{
    core::ClientError,
    types::error::{self, ErrorCode, ErrorObjectOwned},
};

/// An error returned _by the remote server_, not due to e.g serialization errors,
/// protocol errors, or the connection failing.
#[derive(derive_more::From, derive_more::Into, Debug, PartialEq)]
pub struct ServerError {
    inner: ErrorObjectOwned,
}

impl ServerError {
    pub fn new(
        code: i32,
        message: impl Display,
        data: impl Into<Option<serde_json::Value>>,
    ) -> Self {
        Self {
            inner: ErrorObjectOwned::owned(code, message.to_string(), data.into()),
        }
    }
    pub fn message(&self) -> &str {
        self.inner.message()
    }
    pub fn known_code(&self) -> ErrorCode {
        self.inner.code().into()
    }
}

impl Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON-RPC error:\n")?;
        f.write_fmt(format_args!("\tcode: {}\n", self.inner.code()))?;
        f.write_fmt(format_args!("\tmessage: {}\n", self.inner.message()))?;
        if let Some(data) = self.inner.data() {
            f.write_fmt(format_args!("\tdata: {}\n", data))?
        }
        Ok(())
    }
}

impl std::error::Error for ServerError {}

macro_rules! ctor {
    ($($ctor:ident { $code:expr })*) => {
        $(
            impl ServerError {
                pub fn $ctor(message: impl Display, data: impl Into<Option<serde_json::Value>>) -> Self {
                    Self::new($code, message, data)
                }
            }
        )*
    }
}

ctor! {
    parse_error { error::PARSE_ERROR_CODE }
    internal_error { error::INTERNAL_ERROR_CODE }
    invalid_params { error::INVALID_PARAMS_CODE }
    method_not_found { error::METHOD_NOT_FOUND_CODE }
}

macro_rules! from2internal {
    ($($ty:ty),* $(,)?) => {
        $(
            impl From<$ty> for ServerError {
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
    fil_actors_shared::fvm_ipld_hamt::Error,
}

impl From<ServerError> for ClientError {
    fn from(value: ServerError) -> Self {
        Self::Call(value.inner)
    }
}

impl<T> From<flume::SendError<T>> for ServerError {
    fn from(e: flume::SendError<T>) -> Self {
        Self::internal_error(e, None)
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for ServerError {
    fn from(e: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Self::internal_error(e, None)
    }
}

// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::{self, Display};

use crate::rpc::eth::errors::EthErrors;
use jsonrpsee::{
    core::ClientError,
    types::error::{self, ErrorCode, ErrorObjectOwned},
};

/// Trait for errors that can provide additional RPC error data
pub trait RpcErrorData {
    /// Return the error code to use in RPC responses
    fn error_code(&self) -> Option<i32> {
        None
    }

    /// Return the error message to use in RPC responses
    fn error_message(&self) -> Option<String> {
        None
    }

    /// Return additional data to include in the RPC error response
    fn error_data(&self) -> Option<serde_json::Value> {
        None
    }
}

/// An error returned _by the remote server_, not due to e.g serialization errors,
/// protocol errors, or the connection failing.
#[derive(derive_more::From, derive_more::Into, Debug, PartialEq)]
pub struct ServerError {
    inner: ErrorObjectOwned,
}

/// According to the [JSON-RPC 2.0 spec](https://www.jsonrpc.org/specification#response_object),
/// the error codes from -32000 to -32099 are reserved for implementation-defined server-errors.
/// We define them here.
pub(crate) mod implementation_defined_errors {
    /// This error indicates that the method is not supported by the current version of the Forest
    /// node. Note that it's not the same as not found, as we are explicitly not supporting it,
    /// e.g., because it's deprecated or Lotus is doing the same.
    pub(crate) const UNSUPPORTED_METHOD: i32 = -32001;
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
    /// We are only including this method to get the JSON Schemas for our OpenRPC
    /// machinery
    pub fn stubbed_for_openrpc() -> Self {
        Self::new(
            4528,
            "unimplemented",
            Some(
                "This method is stubbed as part of https://github.com/ChainSafe/forest/issues/4528"
                    .into(),
            ),
        )
    }

    pub fn unsupported_method() -> Self {
        Self::new(
            implementation_defined_errors::UNSUPPORTED_METHOD,
            "unsupported method",
            Some("This method is not supported by the current version of the Forest node".into()),
        )
    }

    pub fn inner(&self) -> &ErrorObjectOwned {
        &self.inner
    }
}

impl<E: std::error::Error + RpcErrorData + 'static> From<E> for ServerError {
    fn from(error: E) -> Self {
        let code = error.error_code().unwrap_or(error::INTERNAL_ERROR_CODE);
        let message = error.error_message().unwrap_or_else(|| error.to_string());
        let data = error.error_data();

        Self::new(code, message, data)
    }
}

// Default implementation for anyhow::Error to handle downcasting once
impl From<anyhow::Error> for ServerError {
    fn from(error: anyhow::Error) -> Self {
        // Try to downcast to known RpcErrorData implementations
        if let Some(eth_error) = error.downcast_ref::<EthErrors>() {
            return eth_error.clone().into();
        }

        // Default fallback
        Self::internal_error(error.to_string(), None)
    }
}

impl Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON-RPC error:\n")?;
        f.write_fmt(format_args!("\tcode: {}\n", self.inner.code()))?;
        f.write_fmt(format_args!("\tmessage: {}\n", self.inner.message()))?;
        if let Some(data) = self.inner.data() {
            f.write_fmt(format_args!("\tdata: {data}\n"))?
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

from2internal! {
    String,
    base64::DecodeError,
    cid::multibase::Error,
    crate::chain::store::Error,
    crate::chain_sync::TipsetValidationError,
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
    flume::RecvError,
    fil_actors_shared::v12::ActorError,
    fil_actors_shared::v13::ActorError,
    fil_actors_shared::v14::ActorError,
    fil_actors_shared::v15::ActorError,
    fil_actors_shared::v16::ActorError,
    serde_json::Error,
    jsonrpsee::core::client::error::Error,
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

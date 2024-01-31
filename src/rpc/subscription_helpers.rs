// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::io;

use jsonrpsee::types::error::{
    ErrorCode, ErrorObject, OVERSIZED_RESPONSE_CODE, OVERSIZED_RESPONSE_MSG,
};
use jsonrpsee::types::{Id, InvalidRequest, Response, ResponsePayload, SubscriptionId};
use serde::Serialize;
use serde_json::value::to_raw_value;

#[derive(Debug, Clone)]
pub struct BoundedWriter {
    max_len: usize,
    buf: Vec<u8>,
}

impl BoundedWriter {
    /// Create a new bounded writer.
    pub fn new(max_len: usize) -> Self {
        Self {
            max_len,
            buf: Vec::with_capacity(128),
        }
    }

    /// Consume the writer and extract the written bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

impl<'a> io::Write for &'a mut BoundedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let len = self.buf.len() + buf.len();
        if self.max_len >= len {
            self.buf.extend_from_slice(buf);
            Ok(buf.len())
        } else {
            Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "Memory capacity exceeded",
            ))
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Figure out if this is a sufficiently complete request that we can extract an [`Id`] out of, or just plain
/// unparseable garbage.
pub fn prepare_error(data: &[u8]) -> (Id<'_>, ErrorCode) {
    match serde_json::from_slice::<InvalidRequest>(data) {
        Ok(InvalidRequest { id }) => (id, ErrorCode::InvalidRequest),
        Err(_) => (Id::Null, ErrorCode::ParseError),
    }
}

/// Represents a response to a method call.
///
/// NOTE: A subscription is also a method call but it's
/// possible determine whether a method response
/// is "subscription" or "ordinary method call"
/// by calling [`MethodResponse::is_subscription`]
#[derive(Debug, Clone)]
pub struct MethodResponse {
    /// Serialized JSON-RPC response,
    pub result: String,
    /// Indicates whether the call was successful or not.
    pub success_or_error: MethodResponseResult,
    /// Indicates whether the call was a subscription response.
    pub is_subscription: bool,
}

impl MethodResponse {
    /// Returns whether the call was successful.
    pub fn is_success(&self) -> bool {
        self.success_or_error.is_success()
    }

    /// Returns whether the call failed.
    pub fn is_error(&self) -> bool {
        self.success_or_error.is_success()
    }

    /// Returns whether the call is a subscription.
    pub fn is_subscription(&self) -> bool {
        self.is_subscription
    }
}

/// Represent the outcome of a method call success or failed.
#[derive(Debug, Copy, Clone)]
pub enum MethodResponseResult {
    /// The method call was successful.
    Success,
    /// The method call failed with error code.
    Failed(i32),
}

impl MethodResponseResult {
    /// Returns whether the call was successful.
    pub fn is_success(&self) -> bool {
        matches!(self, MethodResponseResult::Success)
    }

    /// Returns whether the call failed.
    pub fn is_error(&self) -> bool {
        matches!(self, MethodResponseResult::Failed(_))
    }

    /// Get the error code
    ///
    /// Returns `Some(error code)` if the call failed.
    pub fn as_error_code(&self) -> Option<i32> {
        match self {
            Self::Failed(e) => Some(*e),
            _ => None,
        }
    }
}

impl MethodResponse {
    /// This is similar to [`MethodResponse::response`] but sets a flag to indicate
    /// that response is a subscription.
    pub fn subscription_response<T>(
        id: Id,
        result: ResponsePayload<T>,
        max_response_size: usize,
    ) -> Self
    where
        T: Serialize + Clone,
    {
        let mut rp = Self::response(id, result, max_response_size);
        rp.is_subscription = true;
        rp
    }

    /// Create a new method response.
    ///
    /// If the serialization of `result` exceeds `max_response_size` then
    /// the response is changed to an JSON-RPC error object.
    pub fn response<T>(id: Id, result: ResponsePayload<T>, max_response_size: usize) -> Self
    where
        T: Serialize + Clone,
    {
        let mut writer = BoundedWriter::new(max_response_size);

        let success_or_error = if let ResponsePayload::Error(ref e) = result {
            MethodResponseResult::Failed(e.code())
        } else {
            MethodResponseResult::Success
        };

        match serde_json::to_writer(&mut writer, &Response::new(result, id.clone())) {
            Ok(_) => {
                // Safety - serde_json does not emit invalid UTF-8.
                let result = unsafe { String::from_utf8_unchecked(writer.into_bytes()) };

                Self {
                    result,
                    success_or_error,
                    is_subscription: false,
                }
            }
            Err(err) => {
                //tracing::error!(target: LOG_TARGET, "Error serializing response: {:?}", err);

                if err.is_io() {
                    let data =
                        to_raw_value(&format!("Exceeded max limit of {max_response_size}")).ok();
                    let err_code = OVERSIZED_RESPONSE_CODE;

                    let err = ResponsePayload::error_borrowed(ErrorObject::borrowed(
                        err_code,
                        OVERSIZED_RESPONSE_MSG,
                        data.as_deref(),
                    ));
                    let result = serde_json::to_string(&Response::new(err, id))
                        .expect("JSON serialization infallible; qed");

                    Self {
                        result,
                        success_or_error: MethodResponseResult::Failed(err_code),
                        is_subscription: false,
                    }
                } else {
                    let err_code = ErrorCode::InternalError;
                    let result = serde_json::to_string(&Response::new(err_code.into(), id))
                        .expect("JSON serialization infallible; qed");
                    Self {
                        result,
                        success_or_error: MethodResponseResult::Failed(err_code.code()),
                        is_subscription: false,
                    }
                }
            }
        }
    }

    /// This is similar to [`MethodResponse::error`] but sets a flag to indicate
    /// that error is a subscription.
    pub fn subscription_error<'a>(id: Id, err: impl Into<ErrorObject<'a>>) -> Self {
        let mut rp = Self::error(id, err);
        rp.is_subscription = true;
        rp
    }

    /// Create a [`MethodResponse`] from a JSON-RPC error.
    pub fn error<'a>(id: Id, err: impl Into<ErrorObject<'a>>) -> Self {
        let err: ErrorObject = err.into();
        let err_code = err.code();
        let err = ResponsePayload::error_borrowed(err);
        let result = serde_json::to_string(&Response::new(err, id))
            .expect("JSON serialization infallible; qed");
        Self {
            result,
            success_or_error: MethodResponseResult::Failed(err_code),
            is_subscription: false,
        }
    }

    /// Create a close channel method response. This is specific to Filecoin `pubsub`.
    pub fn close_channel_response(channel_id: SubscriptionId) -> Self {
        let channel_str =
            serde_json::to_string(&channel_id).expect("JSON serialization infallible; qed");
        let msg =
            format!(r#"{{"jsonrpc":"2.0","method":"xrpc.ch.close","params":[{channel_str}]}}"#,);
        MethodResponse {
            result: msg,
            success_or_error: MethodResponseResult::Success,
            is_subscription: false,
        }
    }
}

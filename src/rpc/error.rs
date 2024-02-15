// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::chain::store::Error as ChainError;
use crate::key_management::Error as KeyManagementError;
use crate::libp2p::ParseError;
use crate::message_pool::Error as MessagePoolError;
use crate::state_manager::Error as StateManagerError;

use jsonrpsee::types::error::{ErrorObjectOwned, INTERNAL_ERROR_CODE};

pub struct JsonRpcError {
    error: ErrorObjectOwned,
}

impl From<anyhow::Error> for JsonRpcError {
    fn from(e: anyhow::Error) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl From<ErrorObjectOwned> for JsonRpcError {
    fn from(e: ErrorObjectOwned) -> Self {
        Self { error: e }
    }
}

impl From<ChainError> for JsonRpcError {
    fn from(e: ChainError) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl From<MessagePoolError> for JsonRpcError {
    fn from(e: MessagePoolError) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl From<KeyManagementError> for JsonRpcError {
    fn from(e: KeyManagementError) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl From<StateManagerError> for JsonRpcError {
    fn from(e: StateManagerError) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl From<fvm_ipld_encoding::Error> for JsonRpcError {
    fn from(e: fvm_ipld_encoding::Error) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl From<fvm_shared4::address::Error> for JsonRpcError {
    fn from(e: fvm_shared4::address::Error) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl From<fil_actors_shared::fvm_ipld_amt::Error> for JsonRpcError {
    fn from(e: fil_actors_shared::fvm_ipld_amt::Error) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl From<std::io::Error> for JsonRpcError {
    fn from(e: std::io::Error) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl<T> From<flume::SendError<T>> for JsonRpcError {
    fn from(e: flume::SendError<T>) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for JsonRpcError {
    fn from(e: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl From<cid::multibase::Error> for JsonRpcError {
    fn from(e: cid::multibase::Error) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl From<futures::channel::oneshot::Canceled> for JsonRpcError {
    fn from(e: futures::channel::oneshot::Canceled) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl From<jsonwebtoken::errors::Error> for JsonRpcError {
    fn from(e: jsonwebtoken::errors::Error) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl From<base64::DecodeError> for JsonRpcError {
    fn from(e: base64::DecodeError) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl From<tokio::task::JoinError> for JsonRpcError {
    fn from(e: tokio::task::JoinError) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl From<ParseError> for JsonRpcError {
    fn from(e: ParseError) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl From<std::time::SystemTimeError> for JsonRpcError {
    fn from(e: std::time::SystemTimeError) -> Self {
        Self {
            error: ErrorObjectOwned::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None),
        }
    }
}

impl Into<ErrorObjectOwned> for JsonRpcError {
    fn into(self) -> ErrorObjectOwned {
        self.error
    }
}

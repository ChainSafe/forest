// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod bytes;
mod cbor;
mod errors;
mod hash;

pub use serde::{de, ser};
pub use serde_bytes;
pub use serde_cbor::{error, from_reader, from_slice, tags, to_vec, to_writer};

pub use self::bytes::*;
pub use self::cbor::*;
pub use self::errors::*;
pub use self::hash::*;

pub mod tuple {
    pub use serde_tuple::{self, Deserialize_tuple, Serialize_tuple};
}

pub mod repr {
    pub use serde_repr::{Deserialize_repr, Serialize_repr};
}

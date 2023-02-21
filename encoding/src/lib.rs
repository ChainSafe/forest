// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod checked_serde_bytes;
mod hash;
pub mod error {
    pub use serde_ipld_dagcbor::error::{
        DecodeError as CborDecodeError, EncodeError as CborEncodeError,
    };
}

pub use cs_serde_bytes;
pub use serde::{de, ser};

pub use self::{checked_serde_bytes::serde_byte_array, hash::*};

pub mod tuple {
    pub use serde_tuple::{self, Deserialize_tuple, Serialize_tuple};
}

pub mod repr {
    pub use serde_repr::{Deserialize_repr, Serialize_repr};
}
/// lotus use cbor-gen for generating codec for types, it has a length limit for
/// byte array as `2 << 20`
///
/// <https://github.com/whyrusleeping/cbor-gen/blob/f57984553008dd4285df16d4ec2760f97977d713/gen.go#L16>
pub const BYTE_ARRAY_MAX_LEN: usize = 2 << 20;

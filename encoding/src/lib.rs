// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod checked_serde_bytes;
mod hash;

pub use serde::{de, ser};
pub use serde_bytes;
pub use serde_ipld_dagcbor::error;

pub use self::checked_serde_bytes::serde_byte_array;
pub use self::hash::*;

pub use fvm_ipld_encoding::{from_slice, to_vec, Cbor, Error};

pub mod tuple {
    pub use serde_tuple::{self, Deserialize_tuple, Serialize_tuple};
}

pub mod repr {
    pub use serde_repr::{Deserialize_repr, Serialize_repr};
}

pub use fvm_ipld_encoding::{bytes_32, Byte32De, BytesDe, BytesSer};

/// lotus use cbor-gen for generating codec for types, it has a length limit of generic array
/// for `8192`
///
/// https://github.com/whyrusleeping/cbor-gen/blob/f57984553008dd4285df16d4ec2760f97977d713/gen.go#L14
pub const GENERIC_ARRAY_MAX_LEN: usize = 8192;

/// lotus use cbor-gen for generating codec for types, it has a length limit for byte
/// array as `2 << 20`
///
/// https://github.com/whyrusleeping/cbor-gen/blob/f57984553008dd4285df16d4ec2760f97977d713/gen.go#L16
pub const BYTE_ARRAY_MAX_LEN: usize = 2097152;

// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use serde::Serializer;
use serde_bytes::Serialize;

/// checked if output > `crate::ByteArrayMaxLen`
pub fn serialize<T, S>(bytes: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: ?Sized + Serialize,
    S: Serializer,
{
    Serialize::serialize(bytes, serializer)
}

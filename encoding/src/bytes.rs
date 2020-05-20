// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use serde::{Deserialize, Serialize};

/// Wrapper for serializing slice of bytes.
#[derive(Serialize)]
#[serde(transparent)]
pub struct BytesSer<'a>(#[serde(with = "serde_bytes")] pub &'a [u8]);

/// Wrapper for deserializing dynamic sized Bytes.
#[derive(Deserialize)]
#[serde(transparent)]
pub struct BytesDe(#[serde(with = "serde_bytes")] pub Vec<u8>);

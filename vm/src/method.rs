// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_ipld_encoding::RawBytes;

/// Method number indicator for calling actor methods.
pub type MethodNum = u64;

/// Base actor send method.
pub const METHOD_SEND: MethodNum = 0;
/// Base actor constructor method.
pub const METHOD_CONSTRUCTOR: MethodNum = 1;

pub type Serialized = RawBytes;

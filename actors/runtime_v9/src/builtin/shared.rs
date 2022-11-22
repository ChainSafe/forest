// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::runtime::builtins::Type;

pub const HAMT_BIT_WIDTH: u32 = 5;

/// Types of built-in actors that can be treated as principles.
/// This distinction is legacy and should be removed prior to FVM support for
/// user-programmable actors.
pub const CALLER_TYPES_SIGNABLE: &[Type] = &[Type::Account, Type::Multisig];

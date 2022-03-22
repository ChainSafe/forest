// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

mod default_runtime;
mod fvm;
mod gas_block_store;
mod gas_tracker;
mod rand;
mod vm;

pub use self::default_runtime::*;
pub use self::gas_tracker::*;
pub use self::rand::*;
pub use self::vm::*;

/// Temporary flag to switch backends.
/// https://github.com/ChainSafe/forest/pull/1403
/// Run `forest` with flag BACKEND set to `fvm`, `native` or `both`.
/// Defaults to running both backends to compare results.
#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum Backend {
    FVM,
    Native,
    Both,
}

impl Backend {
    pub fn get_backend_choice() -> Backend {
        match std::env::var("BACKEND") {
            Ok(backend) if backend.to_lowercase() == "fvm" => Backend::FVM,
            Ok(backend) if backend.to_lowercase() == "native" => Backend::Native,
            _ => Backend::Both,
        }
    }
}

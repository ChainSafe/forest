// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

mod builtin;
mod singletons;

pub use self::builtin::*;
pub use self::singletons::*;
pub use vm::{ActorID, ActorState, Serialized};

/// Used when invocation requires parameters to be an empty array of bytes
fn assert_empty_params(params: &Serialized) {
    params.deserialize::<[u8; 0]>().unwrap();
}

/// Empty return is an empty serialized array
fn empty_return() -> Serialized {
    Serialized::serialize::<[u8; 0]>([]).unwrap()
}

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

mod builtin;
mod singletons;

pub use self::builtin::*;
pub use self::singletons::*;
pub use vm::{ActorID, ActorState};

use cid::Cid;

// TODO implement Actor for builtin actors on finished spec
/// Actor trait which defines the common functionality of system Actors
pub trait Actor {
    /// Returns Actor Cid
    fn cid(&self) -> &Cid;
    /// Actor public key, if it exists
    fn public_key(&self) -> Vec<u8>;
}

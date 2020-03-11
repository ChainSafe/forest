// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod builtin;

pub use self::builtin::*;
use cid::Cid;

// TODO implement Actor for builtin actors on finished spec
/// Actor trait which defines the common functionality of system Actors
pub trait Actor {
    /// Returns Actor Cid
    fn cid(&self) -> &Cid;
    /// Actor public key, if it exists
    fn public_key(&self) -> Vec<u8>;
}

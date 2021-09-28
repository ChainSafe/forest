// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// workaround for a compiler bug, see https://github.com/rust-lang/rust/issues/55779
extern crate serde;

mod actor_state;
mod deal_id;
mod error;
mod exit_code;
mod method;
mod token;

pub use self::actor_state::*;
pub use self::deal_id::*;
pub use self::error::*;
pub use self::exit_code::*;
pub use self::method::*;
pub use self::token::*;

#[macro_use]
extern crate lazy_static;
use cid::{Cid, Code::Blake2b256};
use encoding::to_vec;

lazy_static! {
    /// Cbor bytes of an empty array serialized.
    pub static ref EMPTY_ARR_BYTES: Vec<u8> = to_vec::<[(); 0]>(&[]).unwrap();

    /// Cid of the empty array Cbor bytes (`EMPTY_ARR_BYTES`).
    pub static ref EMPTY_ARR_CID: Cid = cid::new_from_cbor(&EMPTY_ARR_BYTES, Blake2b256);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_object_checks() {
        assert_eq!(&*EMPTY_ARR_BYTES, &[128u8]);
        assert_eq!(
            EMPTY_ARR_CID.to_string(),
            "bafy2bzacebc3bt6cedhoyw34drrmjvazhu4oj25er2ebk4u445pzycvq4ta4a"
        );
    }
}

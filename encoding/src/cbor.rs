// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use crate::{ser, to_vec};
use cid::{multihash::Hash::Blake2b256, Cid};

/// Implemented for types that are CBOR encodable
pub trait Cbor: ser::Serialize {
    fn marshal_cbor(&self) -> Result<Vec<u8>, Error> {
        Ok(to_vec(&self)?)
    }
    /// returns the content identifier of the block
    fn cid(&self) -> Result<Cid, Error> {
        Ok(Cid::from_bytes(&self.marshal_cbor()?, Blake2b256)?)
    }
}

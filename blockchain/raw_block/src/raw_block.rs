// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use cid::{Cid, Codec, Error, Version};
use encoding::{Cbor, Error as EncodingError};
use multihash::Multihash;

/// Used to extract required encoded data and cid for block and message storage
pub trait RawBlock: Cbor {
    fn raw_data(&self) -> Result<Vec<u8>, EncodingError> {
        self.marshal_cbor()
    }
    /// returns the content identifier of the block
    fn cid(&self) -> Result<Cid, Error> {
        let hash = Multihash::from_bytes(self.raw_data()?)?;
        Ok(Cid::new(Codec::DagCBOR, Version::V1, hash))
    }
}

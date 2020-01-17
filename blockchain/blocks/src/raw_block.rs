// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use cid::Cid;
use encoding::Error as EncodingError;
use multihash::Hash;

// TODO move raw block to own crate
/// Used to extract required encoded data and cid for persistent block storage
pub trait RawBlock {
    fn raw_data(&self) -> Result<Vec<u8>, EncodingError>;
    fn cid(&self) -> Cid;
    fn multihash(&self) -> Hash;
}

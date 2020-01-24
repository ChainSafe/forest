// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use cid::{Cid, Error};
use encoding::Error as EncodingError;

/// Used to extract required encoded data and cid for block and message storage
pub trait RawBlock {
    fn raw_data(&self) -> Result<Vec<u8>, EncodingError>;
    fn cid(&self) -> Result<Cid, Error>;
}

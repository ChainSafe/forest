// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::errors::Error;
use crate::{ser, to_vec};

/// Implemented for types that are CBOR encodable
pub trait Cbor: ser::Serialize {
    fn marshal_cbor(&self) -> Result<Vec<u8>, Error> {
        Ok(to_vec(&self)?)
    }
}

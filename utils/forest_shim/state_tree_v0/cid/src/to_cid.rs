// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{Cid, Error};
use cid::CidGeneric;
use std::convert::TryFrom;
use std::str::FromStr;

impl TryFrom<String> for Cid {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Cid::try_from(value.as_str())
    }
}

impl TryFrom<&str> for Cid {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(Cid(CidGeneric::try_from(value)?))
    }
}

impl FromStr for Cid {
    type Err = Error;

    fn from_str(src: &str) -> Result<Self, Error> {
        Cid::try_from(src)
    }
}

impl TryFrom<Vec<u8>> for Cid {
    type Error = Error;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        Cid::try_from(value.as_slice())
    }
}

impl TryFrom<&[u8]> for Cid {
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Ok(Cid(CidGeneric::try_from(value)?))
    }
}

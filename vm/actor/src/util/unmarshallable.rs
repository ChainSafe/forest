// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::Cbor;
use serde::de::{self, Deserializer};
use serde::ser::{self, Serializer};
use serde::{Deserialize, Serialize};

pub struct UnmarshallableCBOR;

impl Serialize for UnmarshallableCBOR {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Err(ser::Error::custom(
            "Automatic fail when serializing UnmarshallableCBOR",
        ))
    }
}

impl<'de> Deserialize<'de> for UnmarshallableCBOR {
    fn deserialize<D>(_deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        Err(de::Error::custom(
            "Automatic fail when deserializing UnmarshallableCBOR",
        ))
    }
}

impl Cbor for UnmarshallableCBOR {}

#[cfg(test)]
mod tests {

    use super::*;
    use vm::Serialized;
    #[test]
    fn serialize_test() {
        let mut v: Vec<UnmarshallableCBOR> = vec![];

        // Should pass becuase vec is empty
        assert!(Serialized::serialize(&v).is_ok());

        v.push(UnmarshallableCBOR);

        // Should fail becuase vec is no longer empty
        assert!(Serialized::serialize(v).is_err());

        let mut v: Vec<Option<UnmarshallableCBOR>> = vec![];

        v.push(Some(UnmarshallableCBOR));

        // SHould only fail if a actual instance of UnmarshallableCBOR is used
        assert!(Serialized::serialize(v).is_err());
    }
}

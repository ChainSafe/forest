// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::{tuple::*, Cbor};
use serde::de::{self, Deserializer};
use serde::ser::{self, Serializer};
use serde::{Deserialize, Serialize};

#[derive(Default)]
pub struct FailToMarshalCBOR {}

impl Serialize for FailToMarshalCBOR {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Err(ser::Error::custom(
            "Automatic fail when serializing FailToMarshalCBOR",
        ))
    }
}

impl<'de> Deserialize<'de> for FailToMarshalCBOR {
    fn deserialize<D>(_deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        Err(de::Error::custom(
            "Automatic fail when deserializing FailToMarshalCBOR",
        ))
    }
}

impl Cbor for FailToMarshalCBOR {}

#[derive(Default, Serialize_tuple, Deserialize_tuple)]
pub struct State {
    // OptFailToMarshalCBOR is to be used as an Option<T>, with T
    // specialized to FailToMarshalCBOR. If the slice contains no values, the
    // State struct will serialize/deserialize without issue. If the slice contains
    // more than zero values, serialization/deserialization will fail.
    pub opt_fail: Vec<FailToMarshalCBOR>,
}

impl Cbor for State {}

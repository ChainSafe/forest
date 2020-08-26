// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::util::unmarshallable::UnmarshallableCBOR;
use encoding::{tuple::*, Cbor};

#[derive(Default, Serialize_tuple, Deserialize_tuple)]
pub struct State {
    // OptUnmarshallableCBOR is to be used as an Option<T>, with T
    // specialized to UnmarshallableCBOR. If the slice contains no values, the
    // State struct will serialize/deserialize without issue. If the slice contains
    // more than zero values, serialization/deserialization will fail.
    pub opt_fail: Vec<UnmarshallableCBOR>,
}

impl Cbor for State {}

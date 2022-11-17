// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::Cbor;

use crate::util::chaos::unmarshallable::UnmarshallableCBOR;

#[derive(Default, Serialize_tuple, Deserialize_tuple)]
pub struct State {
    // Value can be updated by chaos actor methods to test illegal state
    // mutations when the state is in readonly mode for example.
    pub value: String,

    // Unmarshallable is a sentinel value. If the slice contains no values, the
    // State struct will encode as CBOR without issue. If the slice is non-nil,
    // CBOR encoding will fail.
    pub unmarshallable: Vec<UnmarshallableCBOR>,
}

impl Cbor for State {}

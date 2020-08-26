// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::util::unmarshallable::UnmarshallableCBOR;

pub struct State {
    pub unmarshallable: Vec<UnmarshallableCBOR>,
}

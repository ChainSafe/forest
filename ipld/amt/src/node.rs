// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use cid::Cid;

type Deferred = Vec<u8>;

#[derive(PartialEq, Eq, Clone, Debug, Default)]
pub struct Node<'a> {
    _bmap: Vec<u8>,
    _links: Vec<Cid>,
    _values: Vec<Deferred>, // TODO switch to pointer if necessary

    _exp_links: Vec<Cid>,
    _exp_vals: Vec<Deferred>,
    _cache: Vec<&'a Node<'a>>,
}

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::{Error, TipIndex};
use blocks::Tipset;
use cid::Cid;
use network::service::NetworkMessage;
use num_bigint::BigUint;

pub struct ChainStore {
    // TODO add Blockstore
    // TODO add IPLD Store
    // TODO add StateTreeLoader
    // TODO add access to datastore operations

    // CID of the genesis block.
    _genesis: Cid,

    // Tipset at the head of the best-known chain.
    _head: Tipset,

    // A pubsub channel that publishes an event every time the head changes.
    _notifications: NetworkMessage,

    // tip_index tracks tipsets by epoch/parentset for use by expected consensus.
    _tip_index: TipIndex,
}

impl ChainStore {
    pub fn weight(&self, _ts: &Tipset) -> Result<BigUint, Error> {
        // TODO
        Ok(BigUint::from(0 as u32))
    }
}

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::{Error, TipIndex};
use blocks::{BlockHeader, Tipset};
use cid::Cid;
use db::RocksDb as Blockstore;
use network::service::NetworkMessage;
use num_bigint::BigUint;

pub struct ChainStore {
    // TODO add IPLD Store
    // TODO add StateTreeLoader

    // key-value datastore
    _db: Blockstore,

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
    pub fn persist_headers(&self, _headers: Vec<BlockHeader>) {
        // TODO serialize and put blocks into raw format
        // TODO should be stored as

        //self._db.exists(headers.)
        // self._db.bulk_write(headers.cid(), headers)
    }
}

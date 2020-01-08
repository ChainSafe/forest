// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::TipIndex;
use blocks::Tipset;
use cid::Cid;
use db::RocksDb as Blockstore;
use network::service::NetworkMessage;

pub struct ChainStore {
    // TODO add IPLD Store
    // TODO add StateTreeLoader

    // key-value datastore
    _db: Blockstore,

    // genesis is the CID of the genesis block.
    _genesis: Cid,

    // head is the tipset at the head of the best-known chain.
    _head: Tipset,

    // notifications is a pubsub channel that publishes an event every time the head changes.
    _notifications: NetworkMessage,

    // tip_index tracks tipsets by epoch/parentset for use by expected consensus.
    _tip_index: TipIndex,
}

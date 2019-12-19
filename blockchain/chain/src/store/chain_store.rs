#![allow(dead_code)]
#![allow(unused_variables)]

use super::TipIndex;
use blocks::Tipset;
use cid::Cid;
use network::NetworkMessage;

struct Store {
    // TODO add Blockstore
    // TODO add IPLD Store
    // TODO add StateTreeLoader
    // TODO add access to datastore operations

    // genesis is the CID of the genesis block.
    genesis: Cid,

    // head is the tipset at the head of the best-known chain.
    head: Tipset,

    // notifications is a pubsub channel that publishes an event every time the head changes.
    notifications: NetworkMessage,

    // tip_index tracks tipsets by epoch/parentset for use by expected consensus.
    tip_index: TipIndex,
}

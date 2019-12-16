#![allow(dead_code)]
use super::TipIndex;
use crate::blocks::TipSet;
use crate::network::NetworkMessage;
use cid::Cid;

pub struct Store {
    // TODO add Blockstore
    // TODO add IPLD Store
    // TODO add StateTreeLoader
    // TODO add access to datastore operations

    // genesis is the CID of the genesis block.
    genesis: Cid,

    // head is the tipset at the head of the best-known chain.
    head: TipSet,

    // notifications is a pubsub channel that publishes an event every time the head changes.
    // We operate under the assumption that tipsets published to this channel
    // will always be queued and delivered to subscribers in the order discovered.
    // Successive published tipsets may be supersets of previously published tipsets.
    notifications: NetworkMessage,

    // Tracks tipsets by height/parentset for use by expected consensus.
    tip_index: TipIndex,
}

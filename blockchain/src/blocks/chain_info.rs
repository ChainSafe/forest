#![allow(unused_variables)]
#![allow(dead_code)]

use super::TipSetKeys;
use libp2p::PeerId;
/// BlockMsg is a container used to decode pubsub messages into prior to validation and propagation
pub struct BlockMsg {
    // the originator of the TipSetKey propagation wave
    source: PeerId,
    // the peer that sent us the TipSetKey message
    sender: PeerId,
    // proposed canonical tipset keys
    head: TipSetKeys,
    // proposed chain height
    height: u64,
}

impl BlockMsg {
    /// new creates a BlockMsg container for peer id a head tipset key and chain height
    fn new(source: PeerId, sender: PeerId, head: TipSetKeys, height: u64) -> Self {
        Self {
            source,
            sender,
            head,
            height,
        }
    }
    /// string returns a human-readable string representation of a chain info
    fn string(&self) {
        return println!(
            "source={} sender={} height={} head={:?}",
            self.source, self.sender, self.height, self.head
        );
    }
}

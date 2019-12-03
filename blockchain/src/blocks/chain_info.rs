use libp2p::{
    PeerId
};
use super::TipSetKeys;

struct ChainInfo {
    // the originator of the TipSetKey propagation wave
    source: PeerId,
    // the peer that sent us the TipSetKey message
    sender: PeerId,
    // canonical tipset keys
    head: TipSetKeys,
    // chain height
    height: u64,
}

impl ChainInfo {
    /// new creates a chain info from a peer id a head tipset key and chain height
    fn new(source: PeerId, sender: PeerId, head: TipsettKeys, height: u64) -> Result<Self> {
        Ok(Self {
            source,
            sender,
            head,
            height
        })
    }
    /// string returns a human-readable string representation of a chain info
    fn string(&self) -> String {
        return println!("source={} sender={} height={} head={}", self.source, self.sender, self.height, self.head)
    }
}

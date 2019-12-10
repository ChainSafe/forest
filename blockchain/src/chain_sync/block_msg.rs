use crate::blocks::TipSetKeys;
use libp2p::PeerId;
use std::fmt;

/// BlockMsg is a container used to decode pubsub messages into prior to validation and propagation
/// see https://github.com/filecoin-project/go-filecoin/blob/master/internal/pkg/block/chain_info.go for reference
pub struct BlockMsg {
    // the originator of the TipSetKey propagation wave
    _source: PeerId,
    // the peer that sent us the TipSetKey message
    _sender: PeerId,
    // proposed canonical tipset keys
    _head: TipSetKeys,
    // proposed chain height
    _height: u64,
}

impl BlockMsg {
    /// new creates a BlockMsg container for peer id a head tipset key and chain height
    fn _new(_source: PeerId, _sender: PeerId, _head: TipSetKeys, _height: u64) -> Self {
        Self {
            _source,
            _sender,
            _head,
            _height,
        }
    }
}

/// human-readable string representation of a block msg
impl fmt::Display for BlockMsg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} {} {:?} {}",
            self._source, self._sender, self._head, self._height
        )
    }
}

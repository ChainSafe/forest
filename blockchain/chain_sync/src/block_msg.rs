// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use blocks::TipSetKeys;
use libp2p::PeerId;
use std::fmt;

/// A container used to decode pubsub messages into prior to validation and propagation.
/// See https://github.com/filecoin-project/go-filecoin/blob/master/internal/pkg/block/chain_info.go for reference
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
    /// Creates a BlockMsg container
    fn _new(_source: PeerId, _sender: PeerId, _head: TipSetKeys, _height: u64) -> Self {
        Self {
            _source,
            _sender,
            _head,
            _height,
        }
    }
}

impl fmt::Display for BlockMsg {
    /// Human-readable string representation of a block msg
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} {} {:?} {}",
            self._source, self._sender, self._head, self._height
        )
    }
}

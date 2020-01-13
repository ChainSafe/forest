// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use address::Address;
use clock::ChainEpoch;
use message::{MessageReceipt, SignedMessage, UnsignedMessage};
use state_tree::StateTree;
use std::error::Error;

pub struct VMInterpreter {} // TODO add context necessary
impl VMInterpreter {
    /// Apply all messages from a tipset
    pub fn apply_tip_set_messages(
        _in_tree: &impl StateTree,
        _msgs: TipSetMessages,
    ) -> Result<Vec<MessageReceipt>, Box<dyn Error>> {
        // TODO
        unimplemented!()
    }

    /// Applies the state transition for a single message
    pub fn apply_message(
        _in_tree: &impl StateTree,
        _msg: UnsignedMessage,
        _miner_addr: Address,
    ) -> Result<Vec<MessageReceipt>, Box<dyn Error>> {
        // TODO
        unimplemented!()
    }
}

/// Represents the messages from one block in a tipset.
pub struct BlockMessages {
    _bls_messages: Vec<UnsignedMessage>,
    _secp_messages: Vec<SignedMessage>,
    _miner: Address,      // The block miner's actor address
    _post_proof: Vec<u8>, // The miner's Election PoSt proof output
}

/// Represents the messages from a tipset, grouped by block.
pub struct TipSetMessages {
    _blocks: Vec<BlockMessages>,
    _epoch: ChainEpoch,
}

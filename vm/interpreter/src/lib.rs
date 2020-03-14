// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use blocks::Tipset;
use chain::ChainStore;
use clock::ChainEpoch;
use ipld_blockstore::BlockStore;
use message::{MessageReceipt, SignedMessage, UnsignedMessage};
use vm::{StateTree, TokenAmount};

/// Interpreter which handles execution of state transitioning messages and returns receipts
/// from the vm execution.
pub struct VMInterpreter {} // TODO add context necessary
impl VMInterpreter {
    /// Apply all messages from a tipset
    /// Returns result StateTree and the receipts from the transactions
    pub fn apply_tip_set_messages<ST: StateTree>(
        _in_tree: &ST,
        _tipset: &Tipset,
        _msgs: &TipSetMessages,
    ) -> (ST, Vec<MessageReceipt>) {
        // TODO
        todo!()
    }

    /// Applies the state transition for a single message
    /// Returns result StateTree, receipts from the transaction, and the miner penalty token amount
    pub fn apply_message<DB, ST>(
        _in_tree: &ST,
        _chain: &ChainStore<DB>,
        _msg: &UnsignedMessage,
        _miner_addr: &Address,
    ) -> (ST, MessageReceipt, TokenAmount)
    where
        DB: BlockStore,
        ST: StateTree,
    {
        // TODO
        todo!()
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

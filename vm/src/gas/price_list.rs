// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    MethodNum, PieceInfo, RegisteredProof, SealVerifyInfo, TokenAmount, WindowPoStVerifyInfo,
    METHOD_SEND,
};
use clock::ChainEpoch;
use crypto::SignatureType;
use num_traits::Zero;

/// Provides prices for operations in the VM
#[derive(Copy, Clone, Debug)]
pub struct PriceList {
    /// Gas cost charged to the originator of an on-chain message (regardless of
    /// whether it succeeds or fails in application) is given by:
    ///   OnChainMessageBase + len(serialized message)*OnChainMessagePerByte
    /// Together, these account for the cost of message propagation and validation,
    /// up to but excluding any actual processing by the VM.
    /// This is the cost a block producer burns when including an invalid message.
    pub on_chain_message_base: i64,
    pub on_chain_message_per_byte: i64,

    /// Gas cost charged to the originator of a non-nil return value produced
    /// by an on-chain message is given by:
    ///   len(return value)*OnChainReturnValuePerByte
    pub on_chain_return_value_per_byte: i64,

    /// Gas cost for any message send execution(including the top-level one
    /// initiated by an on-chain message).
    /// This accounts for the cost of loading sender and receiver actors and
    /// (for top-level messages) incrementing the sender's sequence number.
    /// Load and store of actor sub-state is charged separately.
    pub send_base: i64,

    /// Gas cost charged, in addition to SendBase, if a message send
    /// is accompanied by any nonzero currency amount.
    /// Accounts for writing receiver's new balance (the sender's state is
    /// already accounted for).
    pub send_transfer_funds: i64,

    /// Gas cost charged, in addition to SendBase, if a message invokes
    /// a method on the receiver.
    /// Accounts for the cost of loading receiver code and method dispatch.
    pub send_invoke_method: i64,

    /// Gas cost (Base + len*PerByte) for any Get operation to the IPLD store
    /// in the runtime VM context.
    pub ipld_get_base: i64,
    pub ipld_get_per_byte: i64,

    /// Gas cost (Base + len*PerByte) for any Put operation to the IPLD store
    /// in the runtime VM context.
    /// Note: these costs should be significantly higher than the costs for Get
    /// operations, since they reflect not only serialization/deserialization
    /// but also persistent storage of chain data.
    pub ipld_put_base: i64,
    pub ipld_put_per_byte: i64,

    /// Gas cost for creating a new actor (via InitActor's Exec method).
    /// Note: this costs assume that the extra will be partially or totally refunded while
    /// the base is covering for the put.
    pub create_actor_base: i64,
    pub create_actor_extra: i64,

    /// Gas cost for deleting an actor.
    /// Note: this partially refunds the create cost to incentivise the deletion of the actors.
    pub delete_actor: i64,

    pub hashing_base: i64,
    pub hashing_per_byte: i64,

    pub compute_unsealed_sector_cid_base: i64,
    pub verify_seal_base: i64,
    pub verify_post_base: i64,
    pub verify_consensus_fault: i64,
}

impl PriceList {
    /// Returns the gas required for storing a message of a given size in the chain.
    #[inline]
    pub fn on_chain_message(&self, msg_size: i64) -> i64 {
        self.on_chain_message_base + self.on_chain_message_per_byte * msg_size
    }
    /// Returns the gas required for storing the response of a message in the chain.
    #[inline]
    pub fn on_chain_return_value(&self, data_size: usize) -> i64 {
        data_size as i64 * self.on_chain_return_value_per_byte
    }
    /// Returns the gas required when invoking a method.
    #[inline]
    pub fn on_method_invocation(&self, value: &TokenAmount, method_num: MethodNum) -> i64 {
        let mut ret = self.send_base;
        if value != &TokenAmount::zero() {
            ret += self.send_transfer_funds;
        }
        if method_num != METHOD_SEND {
            ret += self.send_invoke_method;
        }
        ret
    }
    /// Returns the gas required for storing an object
    #[inline]
    pub fn on_ipld_get(&self, data_size: usize) -> i64 {
        self.ipld_get_base + data_size as i64 * self.ipld_get_per_byte
    }
    /// Returns the gas required for storing an object
    #[inline]
    pub fn on_ipld_put(&self, data_size: usize) -> i64 {
        self.ipld_put_base + data_size as i64 * self.ipld_put_per_byte
    }
    /// Returns the gas required for creating an actor
    #[inline]
    pub fn on_create_actor(&self) -> i64 {
        self.create_actor_base + self.create_actor_extra
    }
    /// Returns the gas required for deleting an actor
    #[inline]
    pub fn on_delete_actor(&self) -> i64 {
        self.delete_actor
    }
    /// Returns gas required for signature verification
    #[inline]
    pub fn on_verify_signature(&self, sig_type: SignatureType, plain_text_size: usize) -> i64 {
        match sig_type {
            SignatureType::BLS => (3 * plain_text_size + 2) as i64,
            SignatureType::Secp256 => (3 * plain_text_size + 2) as i64,
        }
    }
    /// Returns gas required for hashing data
    #[inline]
    pub fn on_hashing(&self, data_size: usize) -> i64 {
        self.hashing_base + data_size as i64 * self.hashing_per_byte
    }
    /// Returns gas required for computing unsealed sector Cid
    #[inline]
    pub fn on_compute_unsealed_sector_cid(
        &self,
        _proof: RegisteredProof,
        _pieces: &[PieceInfo],
    ) -> i64 {
        self.compute_unsealed_sector_cid_base
    }
    /// Returns gas required for seal verification
    #[inline]
    pub fn on_verify_seal(&self, _info: &SealVerifyInfo) -> i64 {
        self.verify_seal_base
    }
    /// Returns gas required for PoSt verification
    #[inline]
    pub fn on_verify_post(&self, _info: &WindowPoStVerifyInfo) -> i64 {
        self.verify_post_base
    }
    /// Returns gas required for verifying consensus fault
    #[inline]
    pub fn on_verify_consensus_fault(&self) -> i64 {
        self.verify_consensus_fault
    }
}

impl Default for PriceList {
    fn default() -> Self {
        BASE_PRICES
    }
}

const BASE_PRICES: PriceList = PriceList {
    on_chain_message_base: 0,
    on_chain_message_per_byte: 2,
    on_chain_return_value_per_byte: 8,
    send_base: 5,
    send_transfer_funds: 5,
    send_invoke_method: 10,
    ipld_get_base: 10,
    ipld_get_per_byte: 1,
    ipld_put_base: 20,
    ipld_put_per_byte: 2,
    create_actor_base: 40,
    create_actor_extra: 500,
    delete_actor: -500,
    hashing_base: 5,
    hashing_per_byte: 2,
    compute_unsealed_sector_cid_base: 100,
    verify_seal_base: 2000,
    verify_post_base: 700,
    verify_consensus_fault: 10,
};

/// Returns gas price list by Epoch for gas consumption
pub fn price_list_by_epoch(_epoch: ChainEpoch) -> PriceList {
    // In future will match on epoch and select matching price lists when config options allowed
    BASE_PRICES
}

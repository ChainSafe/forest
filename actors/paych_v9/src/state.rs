// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::Cbor;
use fvm_shared::address::Address;

use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;

/// A given payment channel actor is established by `from`
/// to enable off-chain microtransactions to `to` address
/// to be reconciled and tallied on chain.
#[derive(Debug, Serialize_tuple, Deserialize_tuple, Clone)]
pub struct State {
    /// Channel owner, who has funded the actor.
    pub from: Address,
    /// Recipient of payouts from channel.
    pub to: Address,
    /// Amount successfully redeemed through the payment channel, paid out on `Collect`.
    pub to_send: TokenAmount,
    /// Height at which the channel can be collected.
    pub settling_at: ChainEpoch,
    /// Height before which the channel `ToSend` cannot be collected.
    pub min_settle_height: ChainEpoch,
    /// Collections of lane states for the channel, maintained in ID order.
    pub lane_states: Cid, // AMT<LaneState>
}

impl State {
    pub fn new(from: Address, to: Address, empty_arr_cid: Cid) -> Self {
        Self {
            from,
            to,
            to_send: Default::default(),
            settling_at: 0,
            min_settle_height: 0,
            lane_states: empty_arr_cid,
        }
    }
}

/// The Lane state tracks the latest (highest) voucher nonce used to merge the lane
/// as well as the amount it has already redeemed.
#[derive(Default, Clone, PartialEq, Eq, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct LaneState {
    pub redeemed: TokenAmount,
    pub nonce: u64,
}

/// Specifies which `lane`s to be merged with what `nonce` on `channel_update`
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct Merge {
    pub lane: u64,
    pub nonce: u64,
}

impl Cbor for State {}
impl Cbor for LaneState {}
impl Cbor for Merge {}

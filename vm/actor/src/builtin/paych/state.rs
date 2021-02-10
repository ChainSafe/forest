// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use encoding::tuple::*;
use encoding::Cbor;
use num_bigint::{bigint_ser, BigInt};
use vm::TokenAmount;

/// A given payment channel actor is established by `from`
/// to enable off-chain microtransactions to `to` address
/// to be reconciled and tallied on chain.
#[derive(Debug, Serialize_tuple, Deserialize_tuple)]
pub struct State {
    /// Channel owner, who has funded the actor.
    pub from: Address,
    /// Recipient of payouts from channel.
    pub to: Address,
    /// Amount successfully redeemed through the payment channel, paid out on `Collect`.
    #[serde(with = "bigint_ser")]
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
#[derive(Default, Clone, PartialEq, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct LaneState {
    #[serde(with = "bigint_ser")]
    pub redeemed: BigInt,
    pub nonce: u64,
}

/// Specifies which `lane`s to be merged with what `nonce` on `channel_update`
#[derive(Default, Clone, Copy, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct Merge {
    pub lane: usize,
    pub nonce: u64,
}

impl Cbor for State {}
impl Cbor for LaneState {}
impl Cbor for Merge {}

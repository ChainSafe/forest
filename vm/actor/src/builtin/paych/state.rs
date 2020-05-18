// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use clock::ChainEpoch;
use encoding::Cbor;
use num_bigint::{
    bigint_ser::{BigIntDe, BigIntSer},
    biguint_ser::{BigUintDe, BigUintSer},
    BigInt,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::TokenAmount;

/// A given payment channel actor is established by `from`
/// to enable off-chain microtransactions to `to` address
/// to be reconciled and tallied on chain.
#[derive(Debug)]
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
    pub lane_states: Vec<LaneState>,
}

impl State {
    pub fn new(from: Address, to: Address) -> Self {
        Self {
            from,
            to,
            to_send: Default::default(),
            settling_at: 0,
            min_settle_height: 0,
            lane_states: Vec::new(),
        }
    }
}

/// The Lane state tracks the latest (highest) voucher nonce used to merge the lane
/// as well as the amount it has already redeemed.
#[derive(Default, Debug)]
pub struct LaneState {
    /// Identifier unique to this channel
    pub id: u64,
    // TODO this could possibly be a BigUint, but won't affect serialization
    pub redeemed: BigInt,
    pub nonce: u64,
}

/// Specifies which `lane`s to be merged with what `nonce` on `channel_update`
#[derive(Default, Debug, PartialEq)]
pub struct Merge {
    pub lane: u64,
    pub nonce: u64,
}

impl Cbor for State {}
impl Serialize for State {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.from,
            &self.to,
            BigUintSer(&self.to_send),
            &self.settling_at,
            &self.min_settle_height,
            &self.lane_states,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for State {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (from, to, BigUintDe(to_send), settling_at, min_settle_height, lane_states) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            from,
            to,
            to_send,
            settling_at,
            min_settle_height,
            lane_states,
        })
    }
}

impl Cbor for LaneState {}
impl Serialize for LaneState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.id, BigIntSer(&self.redeemed), &self.nonce).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for LaneState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (id, BigIntDe(redeemed), nonce) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            id,
            redeemed,
            nonce,
        })
    }
}

impl Cbor for Merge {}
impl Serialize for Merge {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.lane, &self.nonce).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Merge {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (lane, nonce) = Deserialize::deserialize(deserializer)?;
        Ok(Self { lane, nonce })
    }
}

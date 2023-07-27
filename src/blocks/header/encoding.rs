// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::shim::bigint::{BigIntDe, BigIntSer};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::BlockHeader;

impl Serialize for BlockHeader {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.miner_address,
            &self.ticket,
            &self.election_proof,
            &self.beacon_entries,
            &self.winning_post_proof,
            &self.parents,
            BigIntSer(&self.weight),
            &self.epoch,
            &self.state_root,
            &self.message_receipts,
            &self.messages,
            &self.bls_aggregate,
            &self.timestamp,
            &self.signature,
            &self.fork_signal,
            &self.parent_base_fee,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BlockHeader {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        let (
            miner_address,
            ticket,
            election_proof,
            beacon_entries,
            winning_post_proof,
            parents,
            BigIntDe(weight),
            epoch,
            state_root,
            message_receipts,
            messages,
            bls_aggregate,
            timestamp,
            signature,
            fork_signal,
            parent_base_fee,
        ) = Deserialize::deserialize(deserializer)?;

        let header = BlockHeader {
            parents,
            weight,
            epoch,
            beacon_entries,
            winning_post_proof,
            miner_address,
            messages,
            message_receipts,
            state_root,
            fork_signal,
            signature,
            election_proof,
            timestamp,
            ticket,
            bls_aggregate,
            parent_base_fee,
            cached_cid: Default::default(),
            is_validated: Default::default(),
        };

        Ok(header)
    }
}

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use cid::Cid;
use encoding::de;
use libp2p::PeerId;
use serde::Deserialize;

pub struct StorageMinerActor {}

/// Container representing storage miner actor state
pub struct StorageMinerActorState {
    // TODO add proving_set, post_state
    /// The height at which this miner was slashed at.
    pub slashed_at: u64,
    /// Sectors this miner has committed
    pub sectors: Cid,
    /// Contains static info about this miner
    pub info: Cid,
}

/// Static information about miner
pub struct MinerInfo {
    /// Account that owns this miner
    /// - Income and returned collateral are paid to this address
    /// - This address is also allowed to change the worker address for the miner
    _owner: Address,
    /// Worker account for this miner
    /// This will be the key that is used to sign blocks created by this miner, and
    /// sign messages sent on behalf of this miner to commit sectors, submit PoSts, and
    /// other day to day miner activities
    _worker_address: Address,
    /// Libp2p identity that should be used when connecting to this miner
    _peer_id: PeerId,
    /// Amount of space in each sector committed to the network by this miner
    sector_size: u64,
}

impl MinerInfo {
    /// Returns a reference of the amount of space in each sector committed to the network by this miner
    pub fn sector_size(&self) -> &u64 {
        &self.sector_size
    }
}

impl<'de> de::Deserialize<'de> for StorageMinerActorState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let (slashed_at, sectors, info) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            slashed_at,
            sectors,
            info,
        })
    }
}

impl<'de> de::Deserialize<'de> for MinerInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let (_owner, _worker_address, string_peer_id, sector_size): (_, _, String, _) =
            Deserialize::deserialize(deserializer)?;

        Ok(Self {
            _owner,
            _worker_address,
            sector_size,
            _peer_id: string_peer_id
                .parse()
                .map_err(|_| de::Error::custom("Error parsing PeerId type from String"))?,
        })
    }
}

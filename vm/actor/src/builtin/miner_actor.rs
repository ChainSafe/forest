// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(dead_code)]
use address::Address;
use cid::Cid;
use encoding::de;
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub struct StorageMinerActor {}

/// Container representing storage miner actor state
pub struct StorageMinerActorState {
    // TODO add proving_set, post_state
    /// The height at which this miner was slashed at.
    slashed_at: u64,
    /// Sectors this miner has committed
    sectors: Cid,
    /// Contains static info about this miner
    info: Cid,
}

impl StorageMinerActorState {
    /// Returns reference of epoch which miner was slashed at
    pub fn slashed_at(&self) -> &u64 {
        &self.slashed_at
    }
    /// Returns cid that can be used to retrieve static information regarding this miner
    pub fn info(&self) -> &Cid {
        &self.info
    }
}

/// Static information about miner
pub struct MinerInfo {
    /// Account that owns this miner
    /// - Income and returned collateral are paid to this address
    /// - This address is also allowed to change the worker address for the miner
    owner: Address,
    /// Worker account for this miner
    /// This will be the key that is used to sign blocks created by this miner, and
    /// sign messages sent on behalf of this miner to commit sectors, submit PoSts, and
    /// other day to day miner activities
    worker_address: Address,
    /// Libp2p identity that should be used when connecting to this miner
    peer_id: PeerId,
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

#[derive(Serialize, Deserialize)]
struct MinerSer(Address, Address, String, u64);

impl<'de> de::Deserialize<'de> for MinerInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let MinerSer(owner, worker_address, strtype, sector_size) =
            Deserialize::deserialize(deserializer)?;

        let peer_id = PeerId::from_str(&strtype)
            .map_err(|_| de::Error::custom("Error parsing PeerId type to String"))?;

        Ok(Self {
            owner,
            worker_address,
            peer_id,
            sector_size,
        })
    }
}

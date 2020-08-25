// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::SectorSize;
use address::Address;
use num_bigint::bigint_ser;
use serde::Serialize;
use vm::TokenAmount;

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ActorType {
    Account,
    MultiSig,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Actor {
    pub actor_type: ActorType,
    #[serde(with = "bigint_ser")]
    pub token_amount: TokenAmount,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Miner {
    pub owner: Address,
    pub worker: Address,
    pub peer_id: Vec<u8>,

    #[serde(with = "bigint_ser")]
    pub market_balance: TokenAmount,
    #[serde(with = "bigint_ser")]
    pub power_balance: TokenAmount,
    pub sector_size: SectorSize,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Template {
    pub accounts: Vec<Actor>,
    pub miners: Vec<Miner>,
    pub network_name: String,
    // timestamp: SystemTime,
}

impl Template {
    pub fn new(network_name: String) -> Template {
        Template {
            accounts: Vec::new(),
            miners: Vec::new(),
            network_name,
        }
    }
}

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::SectorSize;
use address::{json as addr_json, Address};
use chrono::Utc;
use num_bigint::bigint_ser::json as bigint_json;
use serde::{Deserialize, Serialize};
use vm::TokenAmount;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActorType {
    Account,
    MultiSig,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Actor {
    pub actor_type: ActorType,
    #[serde(with = "bigint_json")]
    pub balance: TokenAmount,

    #[serde(with = "addr_json")]
    pub owner: Address,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Miner {
    #[serde(with = "addr_json")]
    pub owner: Address,

    #[serde(with = "addr_json")]
    pub worker: Address,
    pub peer_id: String,

    #[serde(with = "bigint_json")]
    pub market_balance: TokenAmount,
    #[serde(with = "bigint_json")]
    pub power_balance: TokenAmount,
    pub sector_size: SectorSize,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Template {
    pub accounts: Vec<Actor>,
    pub miners: Vec<Miner>,
    pub network_name: String,
    timestamp: String,
}

impl Template {
    pub fn new(network_name: String) -> Template {
        Template {
            accounts: Vec::new(),
            miners: Vec::new(),
            network_name,
            timestamp: Utc::now().to_rfc3339(),
        }
    }
}

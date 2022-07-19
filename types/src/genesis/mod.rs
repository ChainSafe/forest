// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_json::address::json as addr_json;
use forest_json::bigint::json as bigint_json;
use forest_vm::TokenAmount;
use fvm_shared::address::Address;
use fvm_shared::sector::SectorSize;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Different account variants. This is used with genesis utils to define the possible
/// genesis allocated actors.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActorType {
    Account,
    MultiSig,
}

/// All information needed to initialize an actor in genesis.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Actor {
    pub actor_type: ActorType,
    #[serde(with = "bigint_json")]
    pub balance: TokenAmount,

    #[serde(with = "addr_json")]
    pub owner: Address,
}

/// Defines all information needed for a miner in genesis.
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

/// Format of genesis file.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Template {
    pub accounts: Vec<Actor>,
    pub miners: Vec<Miner>,
    pub network_name: String,
    #[serde(with = "time::serde::rfc3339")]
    timestamp: OffsetDateTime,
}

impl Template {
    pub fn new(network_name: String) -> Template {
        Template {
            accounts: Vec::new(),
            miners: Vec::new(),
            network_name,
            timestamp: OffsetDateTime::now_utc(),
        }
    }
}

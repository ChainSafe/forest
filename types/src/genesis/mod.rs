// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::SectorSize;
use address::Address;
use num_bigint::bigint_ser;
use serde::Serialize;
use vm::TokenAmount;

#[derive(Serialize)]
enum ActorType {
    // Account,
// MultiSig,
}

#[derive(Serialize)]
pub struct Actor {
    actor_type: ActorType,
    #[serde(with = "bigint_ser")]
    token_amount: TokenAmount,
}

#[derive(Serialize)]
pub struct Miner {
    owner: Address,
    worker: Address,
    peer_id: Vec<u8>,

    #[serde(with = "bigint_ser")]
    market_balance: TokenAmount,
    #[serde(with = "bigint_ser")]
    power_balance: TokenAmount,
    sector_size: SectorSize,
}

#[derive(Serialize)]
pub struct Template {
    accounts: Vec<Actor>,
    miners: Vec<Miner>,
    network_name: String,
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

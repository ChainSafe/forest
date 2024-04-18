// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::beacon::BeaconEntry;
use crate::lotus_json::{lotus_json_with_self, LotusJson};
use crate::shim::{
    address::Address,
    econ::TokenAmount,
    error::ExitCode,
    executor::Receipt,
    message::Message,
    sector::{SectorInfo, StoragePower},
    state_tree::{ActorID, ActorState},
};
use cid::Cid;
use fvm_ipld_encoding::RawBytes;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ApiInvocResult {
    #[serde(with = "crate::lotus_json")]
    pub msg: Message,
    #[serde(with = "crate::lotus_json")]
    pub msg_cid: Cid,
    #[serde(with = "crate::lotus_json")]
    pub msg_rct: Option<Receipt>,
    pub error: String,
    pub duration: u64,
    #[serde(with = "crate::lotus_json")]
    pub gas_cost: MessageGasCost,
    #[serde(with = "crate::lotus_json")]
    pub execution_trace: Option<ExecutionTrace>,
}

lotus_json_with_self!(ApiInvocResult);

impl PartialEq for ApiInvocResult {
    /// Ignore [`Self::duration`] as it is implementation-dependent
    fn eq(&self, other: &Self) -> bool {
        self.msg == other.msg
            && self.msg_cid == other.msg_cid
            && self.msg_rct == other.msg_rct
            && self.error == other.error
            && self.gas_cost == other.gas_cost
            && self.execution_trace == other.execution_trace
    }
}

#[derive(Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageGasCost {
    #[serde(with = "crate::lotus_json")]
    pub message: Option<Cid>,
    #[serde(with = "crate::lotus_json")]
    pub gas_used: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub base_fee_burn: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub over_estimation_burn: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub miner_penalty: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub miner_tip: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub refund: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    pub total_cost: TokenAmount,
}

lotus_json_with_self!(MessageGasCost);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ExecutionTrace {
    #[serde(with = "crate::lotus_json")]
    pub msg: MessageTrace,
    #[serde(with = "crate::lotus_json")]
    pub msg_rct: ReturnTrace,
    #[serde(with = "crate::lotus_json")]
    pub invoked_actor: Option<ActorTrace>,
    #[serde(with = "crate::lotus_json")]
    pub gas_charges: Vec<GasTrace>,
    #[serde(with = "crate::lotus_json")]
    pub subcalls: Vec<ExecutionTrace>,
}

lotus_json_with_self!(ExecutionTrace);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageTrace {
    #[serde(with = "crate::lotus_json")]
    pub from: Address,
    #[serde(with = "crate::lotus_json")]
    pub to: Address,
    #[serde(with = "crate::lotus_json")]
    pub value: TokenAmount,
    pub method: u64,
    #[serde(with = "crate::lotus_json")]
    pub params: RawBytes,
    pub params_codec: u64,
    pub gas_limit: Option<u64>,
    pub read_only: Option<bool>,
}

lotus_json_with_self!(MessageTrace);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActorTrace {
    pub id: ActorID,
    #[serde(with = "crate::lotus_json")]
    pub state: ActorState,
}

lotus_json_with_self!(ActorTrace);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ReturnTrace {
    pub exit_code: ExitCode,
    #[serde(with = "crate::lotus_json")]
    pub r#return: RawBytes,
    pub return_codec: u64,
}

lotus_json_with_self!(ReturnTrace);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GasTrace {
    pub name: String,
    #[serde(rename = "tg")]
    pub total_gas: u64,
    #[serde(rename = "cg")]
    pub compute_gas: u64,
    #[serde(rename = "sg")]
    pub storage_gas: u64,
    #[serde(rename = "tt")]
    pub time_taken: u64,
}

lotus_json_with_self!(GasTrace);

impl PartialEq for GasTrace {
    /// Ignore [`Self::total_gas`] as it is implementation-dependent
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.total_gas == other.total_gas
            && self.compute_gas == other.compute_gas
            && self.storage_gas == other.storage_gas
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct MiningBaseInfo {
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<StoragePower>")]
    pub miner_power: StoragePower,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<StoragePower>")]
    pub network_power: StoragePower,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Vec<SectorInfo>>")]
    pub sectors: Vec<SectorInfo>,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Address>")]
    pub worker_key: Address,
    #[schemars(with = "u64")]
    pub sector_size: fvm_shared2::sector::SectorSize,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<BeaconEntry>")]
    pub prev_beacon_entry: BeaconEntry,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Vec<BeaconEntry>>")]
    pub beacon_entries: Vec<BeaconEntry>,
    pub eligible_for_mining: bool,
}

lotus_json_with_self!(MiningBaseInfo);

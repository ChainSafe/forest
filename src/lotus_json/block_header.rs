// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::beacon::BeaconEntry;
use crate::lotus_json::*;
use crate::shim::sector::PoStProof;
use crate::{
    blocks::{ElectionProof, Ticket, TipsetKey},
    shim::address::Address,
    shim::{crypto::Signature, econ::TokenAmount},
};
use ::cid::Cid;
use num::BigInt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::blocks::{CachingBlockHeader, RawBlockHeader};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "BlockHeader")]
pub struct BlockHeaderLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    miner: Address,
    #[schemars(with = "LotusJson<Option<Ticket>>")]
    #[serde(
        with = "crate::lotus_json",
        skip_serializing_if = "Option::is_none",
        default
    )]
    ticket: Option<Ticket>,
    #[schemars(with = "LotusJson<Option<ElectionProof>>")]
    #[serde(
        with = "crate::lotus_json",
        skip_serializing_if = "Option::is_none",
        default
    )]
    election_proof: Option<ElectionProof>,
    #[schemars(with = "LotusJson<Vec<BeaconEntry>>")]
    #[serde(with = "crate::lotus_json")]
    beacon_entries: Vec<BeaconEntry>,
    #[schemars(with = "LotusJson<Vec<PoStProof>>")]
    #[serde(with = "crate::lotus_json")]
    win_po_st_proof: Vec<PoStProof>,
    #[schemars(with = "LotusJson<TipsetKey>")]
    #[serde(with = "crate::lotus_json")]
    parents: TipsetKey,
    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    parent_weight: BigInt,
    height: i64,
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    parent_state_root: Cid,
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    parent_message_receipts: Cid,
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    messages: Cid,
    #[schemars(with = "LotusJson<Option<Signature>>")]
    #[serde(
        with = "crate::lotus_json",
        skip_serializing_if = "Option::is_none",
        default
    )]
    b_l_s_aggregate: Option<Signature>,
    timestamp: u64,
    #[schemars(with = "LotusJson<Option<Signature>>")]
    #[serde(
        with = "crate::lotus_json",
        skip_serializing_if = "Option::is_none",
        default
    )]
    block_sig: Option<Signature>,
    fork_signaling: u64,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    parent_base_fee: TokenAmount,
}

impl HasLotusJson for CachingBlockHeader {
    type LotusJson = BlockHeaderLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        use serde_json::json;

        vec![(
            json!({
                "BeaconEntries": null,
                "Miner": "f00",
                "Parents": [{"/":"bafyreiaqpwbbyjo4a42saasj36kkrpv4tsherf2e7bvezkert2a7dhonoi"}],
                "ParentWeight": "0",
                "Height": 0,
                "ParentStateRoot": {
                    "/": "baeaaaaa"
                },
                "ParentMessageReceipts": {
                    "/": "baeaaaaa"
                },
                "Messages": {
                    "/": "baeaaaaa"
                },
                "WinPoStProof": null,
                "Timestamp": 0,
                "ForkSignaling": 0,
                "ParentBaseFee": "0",
            }),
            CachingBlockHeader::default(),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let RawBlockHeader {
            miner_address,
            ticket,
            election_proof,
            beacon_entries,
            winning_post_proof,
            parents,
            weight,
            epoch,
            state_root,
            message_receipts,
            messages,
            bls_aggregate,
            timestamp,
            signature,
            fork_signal,
            parent_base_fee,
        } = self.into_raw();
        Self::LotusJson {
            miner: miner_address,
            ticket,
            election_proof,
            beacon_entries,
            win_po_st_proof: winning_post_proof,
            parents,
            parent_weight: weight,
            height: epoch,
            parent_state_root: state_root,
            parent_message_receipts: message_receipts,
            messages,
            b_l_s_aggregate: bls_aggregate,
            timestamp,
            block_sig: signature,
            fork_signaling: fork_signal,
            parent_base_fee,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            miner,
            ticket,
            election_proof,
            beacon_entries,
            win_po_st_proof,
            parents,
            parent_weight,
            height,
            parent_state_root,
            parent_message_receipts,
            messages,
            b_l_s_aggregate,
            timestamp,
            block_sig,
            fork_signaling,
            parent_base_fee,
        } = lotus_json;
        Self::new(RawBlockHeader {
            parents,
            weight: parent_weight,
            epoch: height,
            beacon_entries,
            winning_post_proof: win_po_st_proof,
            miner_address: miner,
            messages,
            message_receipts: parent_message_receipts,
            state_root: parent_state_root,
            fork_signal: fork_signaling,
            signature: block_sig,
            election_proof,
            timestamp,
            ticket,
            bls_aggregate: b_l_s_aggregate,
            parent_base_fee,
        })
    }
}

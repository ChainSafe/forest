// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::beacon::BeaconEntry;
use crate::lotus_json::*;
use crate::shim::sector::PoStProof;
use crate::{
    blocks::{ElectionProof, Ticket, TipsetKeys},
    shim::address::Address,
    shim::{crypto::Signature, econ::TokenAmount},
};
use cid::Cid;
use num::BigInt;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

use super::BlockHeader;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BlockHeaderLotusJson {
    miner: LotusJson<Address>,
    #[serde(skip_serializing_if = "LotusJson::is_none", default)]
    ticket: LotusJson<Option<Ticket>>,
    #[serde(skip_serializing_if = "LotusJson::is_none", default)]
    election_proof: LotusJson<Option<ElectionProof>>,
    beacon_entries: LotusJson<Vec<BeaconEntry>>,
    win_po_st_proof: LotusJson<Vec<PoStProof>>,
    parents: LotusJson<TipsetKeys>,
    parent_weight: LotusJson<BigInt>,
    height: LotusJson<i64>,
    parent_state_root: LotusJson<Cid>,
    parent_message_receipts: LotusJson<Cid>,
    messages: LotusJson<Cid>,
    #[serde(skip_serializing_if = "LotusJson::is_none", default)]
    b_l_s_aggregate: LotusJson<Option<Signature>>,
    timestamp: LotusJson<u64>,
    #[serde(skip_serializing_if = "LotusJson::is_none", default)]
    block_sig: LotusJson<Option<Signature>>,
    fork_signaling: LotusJson<u64>,
    parent_base_fee: LotusJson<TokenAmount>,
}

impl HasLotusJson for BlockHeader {
    type LotusJson = BlockHeaderLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        use serde_json::json;

        vec![(
            json!({
                "BeaconEntries": null,
                "Miner": "f00",
                "Parents": null,
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
            BlockHeader::default(),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let Self {
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
            cached_cid: _ignore_cache0,
            is_validated: _ignore_cache1,
        } = self;
        Self::LotusJson {
            miner: miner_address.into(),
            ticket: ticket.into(),
            election_proof: election_proof.into(),
            beacon_entries: beacon_entries.into(),
            win_po_st_proof: winning_post_proof.into(),
            parents: parents.into(),
            parent_weight: weight.into(),
            height: epoch.into(),
            parent_state_root: state_root.into(),
            parent_message_receipts: message_receipts.into(),
            messages: messages.into(),
            b_l_s_aggregate: bls_aggregate.into(),
            timestamp: timestamp.into(),
            block_sig: signature.into(),
            fork_signaling: fork_signal.into(),
            parent_base_fee: parent_base_fee.into(),
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
        Self {
            parents: parents.into_inner(),
            weight: parent_weight.into_inner(),
            epoch: height.into_inner(),
            beacon_entries: beacon_entries.into_inner(),
            winning_post_proof: win_po_st_proof.into_inner(),
            miner_address: miner.into_inner(),
            messages: messages.into_inner(),
            message_receipts: parent_message_receipts.into_inner(),
            state_root: parent_state_root.into_inner(),
            fork_signal: fork_signaling.into_inner(),
            signature: block_sig.into_inner(),
            election_proof: election_proof.into_inner(),
            timestamp: timestamp.into_inner(),
            ticket: ticket.into_inner(),
            bls_aggregate: b_l_s_aggregate.into_inner(),
            parent_base_fee: parent_base_fee.into_inner(),
            cached_cid: OnceCell::new(),
            is_validated: OnceCell::new(),
        }
    }
}

#[test]
fn snapshots() {
    assert_all_snapshots::<BlockHeader>()
}

#[cfg(test)]
quickcheck::quickcheck! {
    fn quickcheck(val: BlockHeader) -> () {
        assert_unchanged_via_json(val)
    }
}

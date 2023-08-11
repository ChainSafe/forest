// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::lotus_json::*;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

use super::BlockHeader;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BlockHeaderLotusJson {
    miner: AddressLotusJson,
    #[serde(skip_serializing_if = "Option::is_none")]
    ticket: Option<TicketLotusJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    election_proof: Option<ElectionProofLotusJson>,
    beacon_entries: VecLotusJson<BeaconEntryLotusJson>,
    win_po_st_proof: VecLotusJson<PoStProofLotusJson>,
    parents: TipsetKeysLotusJson,
    parent_weight: BigIntLotusJson,
    height: i64,
    parent_state_root: CidLotusJson,
    parent_message_receipts: CidLotusJson,
    messages: CidLotusJson,
    #[serde(skip_serializing_if = "Option::is_none")]
    b_l_s_aggregate: Option<SignatureLotusJson>,
    timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    block_sig: Option<SignatureLotusJson>,
    fork_signaling: u64,
    parent_base_fee: TokenAmountLotusJson,
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

impl From<BlockHeader> for BlockHeaderLotusJson {
    fn from(value: BlockHeader) -> Self {
        let BlockHeader {
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
        } = value;
        Self {
            miner: miner_address.into(),
            ticket: ticket.map(Into::into),
            election_proof: election_proof.map(Into::into),
            beacon_entries: beacon_entries.into(),
            win_po_st_proof: winning_post_proof.into(),
            parents: parents.into(),
            parent_weight: weight.into(),
            height: epoch,
            parent_state_root: state_root.into(),
            parent_message_receipts: message_receipts.into(),
            messages: messages.into(),
            b_l_s_aggregate: bls_aggregate.map(Into::into),
            timestamp,
            block_sig: signature.map(Into::into),
            fork_signaling: fork_signal,
            parent_base_fee: parent_base_fee.into(),
        }
    }
}

impl From<BlockHeaderLotusJson> for BlockHeader {
    fn from(value: BlockHeaderLotusJson) -> Self {
        let BlockHeaderLotusJson {
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
            b_l_s_aggregate: bls_aggregate,
            timestamp,
            block_sig,
            fork_signaling,
            parent_base_fee,
        } = value;
        Self {
            parents: parents.into(),
            weight: parent_weight.into(),
            epoch: height,
            beacon_entries: beacon_entries.into(),
            winning_post_proof: win_po_st_proof.into(),
            miner_address: miner.into(),
            messages: messages.into(),
            message_receipts: parent_message_receipts.into(),
            state_root: parent_state_root.into(),
            fork_signal: fork_signaling,
            signature: block_sig.map(Into::into),
            election_proof: election_proof.map(Into::into),
            timestamp,
            ticket: ticket.map(Into::into),
            bls_aggregate: bls_aggregate.map(Into::into),
            parent_base_fee: parent_base_fee.into(),
            cached_cid: OnceCell::new(),
            is_validated: OnceCell::new(),
        }
    }
}

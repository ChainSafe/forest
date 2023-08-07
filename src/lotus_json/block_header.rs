use crate::blocks::BlockHeader;

use super::*;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BlockHeaderLotusJson {
    miner: String,
    ticket: Option<TicketLotusJson>,
    election_proof: Option<ElectionProofLotusJson>,
    beacon_entries: VecLotusJson<BeaconEntryLotusJson>,
    win_po_st_proof: VecLotusJson<PoStProofLotusJson>,
    parents: TipsetKeysLotusJson,
    parent_weight: String,
    height: i64,
    parent_state_root: CidLotusJson,
    parent_message_receipts: CidLotusJson,
    messages: CidLotusJson,
    #[serde(rename = "BLSAggregate")]
    bls_aggregate: Option<SignatureLotusJson>,
    timestamp: u64,
    block_sig: Option<SignatureLotusJson>,
    fork_signaling: u64,
    parent_base_fee: String,
}

impl HasLotusJson for BlockHeader {
    type LotusJson = BlockHeaderLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        todo!()
    }
}

impl From<BlockHeader> for BlockHeaderLotusJson {
    fn from(value: BlockHeader) -> Self {
        todo!()
    }
}

impl From<BlockHeaderLotusJson> for BlockHeader {
    fn from(value: BlockHeaderLotusJson) -> Self {
        todo!()
    }
}

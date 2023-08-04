#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct BlockHeaderSer<'a> {
    miner: String,
    #[serde(with = "ticket::json::opt")]
    ticket: &'a Option<Ticket>,
    #[serde(with = "election_proof::json::opt")]
    election_proof: &'a Option<ElectionProof>,
    #[serde(with = "crate::json::empty_slice_is_null", borrow)]
    beacon_entries: &'a [BeaconEntry],
    #[serde(rename = "WinPoStProof", with = "crate::json::empty_slice_is_null")]
    winning_post_proof: &'a [PoStProof],
    #[serde(rename = "Parents", with = "tipset_keys_json")]
    parents: &'a TipsetKeys,
    #[serde(rename = "ParentWeight")]
    weight: String,
    height: &'a i64,
    #[serde(rename = "ParentStateRoot", with = "crate::json::cid")]
    state_root: &'a Cid,
    #[serde(rename = "ParentMessageReceipts", with = "crate::json::cid")]
    message_receipts: &'a Cid,
    #[serde(with = "crate::json::cid")]
    messages: &'a Cid,
    #[serde(rename = "BLSAggregate", with = "signature::json::opt")]
    bls_aggregate: &'a Option<Signature>,
    timestamp: &'a u64,
    #[serde(rename = "BlockSig", with = "signature::json::opt")]
    signature: &'a Option<Signature>,
    #[serde(rename = "ForkSignaling")]
    fork_signal: &'a u64,
    parent_base_fee: String,
}

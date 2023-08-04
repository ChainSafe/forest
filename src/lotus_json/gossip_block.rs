#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct GossipBlockLotusJson {
    #[serde(with = "header::json")]
    pub header: BlockHeader,
    #[serde(with = "crate::json::empty_vec_is_null")]
    pub bls_messages: Vec<Cid>,
    #[serde(with = "crate::json::empty_vec_is_null")]
    pub secpk_messages: Vec<Cid>,
}

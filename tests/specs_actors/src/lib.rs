pub struct TestVector {
    pub class: String,
    #[serde(rename = "_meta")]
    pub meta: Metadata,
    #[serde(with = "base64_bytes")]
    pub car: Vec<u8>,
    pub pre_conditions: PreConditions,
    pub apply_messages: Vec<AppleMessage>,
    pub post_conditions: PostConditions, 
}

#[derive(Debug, Deserialize, Clone)]
pub struct ApplyMessage {
   #[serde(with = "base64_bytes")]
   pub bytes: Vec<u8>,
}


#[derive(Debug, Deserialize, Clone)]
pub struct Metadata {
    pub id: String,
    pub gen: Vec<GenData>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GenData {
    pub source: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PreConditions {
    pub variants: Vec<Variant>,
    pub state_tree: StateTree,
    pub base_fee: u64,
    pub circ_supply: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PostConditions {
    pub state_tree: StateTree,
    pub receipts: Vec<MessageReceipt>,
}

#[derive(Debug, Serialize, Clone)]
pub struct StateTree {
    #[serde(with = "cid::json")]
    pub root_cid: Cid,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MessageReceipt {
    pub exit_code: ExitCode,
    #[serde(rename = "return", with = "base64_bytes")]
    pub return_value: Vec<u8>,
    pub gas_used: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Variant {
    pub id: String,
    pub epoch: ChainEpoch,
    pub nv: u32,    
}

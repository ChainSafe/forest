// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use cid::multihash::MultihashDigest;
use cid::Cid;

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Eq, PartialEq, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ObjStat {
    pub size: usize,
    pub links: usize,
}
lotus_json_with_self!(ObjStat);

pub trait Block {
    fn raw_data(&self) -> &[u8];
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Eq, PartialEq, Default)]
#[serde(rename_all = "PascalCase")]
pub struct BasicBlock {
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Cid>")]
    cid: Cid,
    data: Vec<u8>,
}

impl BasicBlock {
    pub fn new(data: Vec<u8>) -> Self {
        let hash = cid::multihash::Code::Sha2_256.digest(&data);
        let cid = Cid::new_v0(hash).expect("Failed to create CID");
        BasicBlock { cid, data }
    }
}

impl Block for BasicBlock {
    fn raw_data(&self) -> &[u8] {
        &self.data
    }
}

lotus_json_with_self!(BasicBlock);

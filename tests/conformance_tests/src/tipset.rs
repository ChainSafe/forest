// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use state_manager::StateManager;
use std::sync::Arc;

mod block_messages_json {
    use super::*;
    use serde::de;

    #[derive(Deserialize)]
    pub struct BlockMessageJson {
        #[serde(with = "address::json")]
        pub miner_addr: Address,
        pub win_count: i64,
        #[serde(with = "base64_bytes::vec")]
        pub messages: Vec<Vec<u8>>,
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<BlockMessages>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bm: Vec<BlockMessageJson> = Deserialize::deserialize(deserializer)?;
        Ok(bm
            .into_iter()
            .map(|m| {
                let mut secpk_messages = Vec::new();
                let mut bls_messages = Vec::new();
                for message in &m.messages {
                    match ChainMessage::unmarshal_cbor(message).map_err(de::Error::custom)? {
                        ChainMessage::Signed(s) => secpk_messages.push(s),
                        ChainMessage::Unsigned(u) => bls_messages.push(u),
                    }
                }
                Ok(BlockMessages {
                    miner: m.miner_addr,
                    win_count: m.win_count,
                    bls_messages,
                    secpk_messages,
                })
            })
            .collect::<Result<Vec<BlockMessages>, _>>()?)
    }
}

#[derive(Debug, Deserialize)]
pub struct TipsetVector {
    pub epoch: ChainEpoch,
    #[serde(with = "bigint_json")]
    pub basefee: BigInt,
    #[serde(with = "block_messages_json")]
    pub blocks: Vec<BlockMessages>,
}

pub struct ExecuteTipsetResult {
    pub receipts_root: Cid,
    pub post_state_root: Cid,
    pub _applied_messages: Vec<UnsignedMessage>,
    pub applied_results: Vec<ApplyRet>,
}

pub fn execute_tipset(
    bs: Arc<db::MemoryDB>,
    pre_root: &Cid,
    parent_epoch: ChainEpoch,
    tipset: &TipsetVector,
) -> Result<ExecuteTipsetResult, Box<dyn StdError>> {
    let sm = StateManager::new(bs);
    let mut _applied_messages = Vec::new();
    let mut applied_results = Vec::new();
    let (post_state_root, receipts_root) = sm.apply_blocks(
        parent_epoch,
        pre_root,
        &tipset.blocks,
        tipset.epoch,
        &TestRand,
        tipset.basefee.clone(),
        Some(|_, msg, ret| {
            _applied_messages.push(msg);
            applied_results.push(ret);
            Ok(())
        }),
    )?;
    Ok(ExecuteTipsetResult {
        receipts_root,
        post_state_root,
        _applied_messages,
        applied_results,
    })
}

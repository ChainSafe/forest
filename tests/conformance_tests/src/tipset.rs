// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use fil_types::verifier::MockVerifier;
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
                        m @ ChainMessage::Signed(_) => secpk_messages.push(m),
                        m @ ChainMessage::Unsigned(_) => bls_messages.push(m),
                    }
                }
                bls_messages.append(&mut secpk_messages);
                Ok(BlockMessages {
                    miner: m.miner_addr,
                    win_count: m.win_count,
                    messages: bls_messages,
                })
            })
            .collect::<Result<Vec<BlockMessages>, _>>()?)
    }
}

#[derive(Debug, Deserialize)]
pub struct TipsetVector {
    pub epoch: ChainEpoch,
    pub basefee: u64,
    #[serde(with = "block_messages_json")]
    pub blocks: Vec<BlockMessages>,
}

pub struct ExecuteTipsetResult {
    pub receipts_root: Cid,
    pub post_state_root: Cid,
    pub _applied_messages: Vec<ChainMessage>,
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
    let (post_state_root, receipts_root) = sm.apply_blocks::<_, MockVerifier, _>(
        parent_epoch,
        pre_root,
        &tipset.blocks,
        tipset.epoch,
        &TestRand,
        BigInt::from(tipset.basefee),
        Some(|_, msg: &ChainMessage, ret| {
            _applied_messages.push(msg.clone());
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

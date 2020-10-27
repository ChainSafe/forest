// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use fil_types::verifier::FullVerifier;
use num_bigint::ToBigInt;
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
                    let msg_decoded =
                        UnsignedMessage::unmarshal_cbor(&message).map_err(de::Error::custom)?;
                    match msg_decoded.from().protocol() {
                        Protocol::Secp256k1 => secpk_messages.push(to_chain_msg(msg_decoded)),
                        Protocol::BLS => bls_messages.push(to_chain_msg(msg_decoded)),
                        _ => {
                            // matching go runner to force failure (bad address)
                            secpk_messages.push(to_chain_msg(msg_decoded.clone()));
                            bls_messages.push(to_chain_msg(msg_decoded));
                        }
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
    pub epoch_offset: ChainEpoch,
    pub basefee: f64,
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
    exec_epoch: ChainEpoch,
) -> Result<ExecuteTipsetResult, Box<dyn StdError>> {
    let sm = StateManager::new(bs);
    let mut _applied_messages = Vec::new();
    let mut applied_results = Vec::new();
    let (post_state_root, receipts_root) = sm.apply_blocks::<_, FullVerifier, _>(
        parent_epoch,
        pre_root,
        &tipset.blocks,
        exec_epoch,
        &TestRand,
        tipset.basefee.to_bigint().unwrap_or_default(),
        Some(|_: &Cid, msg: &ChainMessage, ret: &ApplyRet| {
            _applied_messages.push(msg.clone());
            applied_results.push(ret.clone());
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

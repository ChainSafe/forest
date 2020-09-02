// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "submodule_tests")]

#[macro_use]
extern crate lazy_static;

use actor::{CHAOS_ACTOR_CODE_ID, PUPPET_ACTOR_CODE_ID};
use address::Address;
use blockstore::BlockStore;
use cid::Cid;
use clock::ChainEpoch;
use crypto::{DomainSeparationTag, Signature};
use encoding::Cbor;
use fil_types::{SealVerifyInfo, WindowPoStVerifyInfo};
use flate2::read::GzDecoder;
use forest_message::{ChainMessage, MessageReceipt, UnsignedMessage};
use interpreter::{ApplyRet, BlockMessages, Rand, VM};
use num_bigint::BigInt;
use regex::Regex;
use runtime::{ConsensusFault, Syscalls};
use serde::{Deserialize, Deserializer};
use state_manager::StateManager;
use std::error::Error as StdError;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use vm::{ExitCode, Serialized, TokenAmount};
use walkdir::{DirEntry, WalkDir};

lazy_static! {
    static ref SKIP_TESTS: [Regex; 5] = [
        // These tests are marked as invalid as they return wrong exit code on Lotus
        Regex::new(r"actor_creation/x--params*").unwrap(),
        // Following two fail for the same invalid exit code return
        Regex::new(r"nested/nested_sends--fail-missing-params.json").unwrap(),
        Regex::new(r"nested/nested_sends--fail-mismatch-params.json").unwrap(),
        // Lotus client does not fail in inner transaction for insufficient funds
        Regex::new(r"test-vectors/corpus/nested/nested_sends--fail-insufficient-funds-for-transfer-in-inner-send.json").unwrap(),
        // TODO this is the tipset vector that should pass and this should be removed
        Regex::new(r"test-vectors/corpus/reward/reward--ok-miners-awarded-no-premiums.json").unwrap(),
    ];
    static ref BASE_FEE: TokenAmount = TokenAmount::from(100);
}

mod base64_bytes {
    use super::*;
    use serde::de;
    use std::borrow::Cow;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
        Ok(base64::decode(s.as_ref()).map_err(de::Error::custom)?)
    }

    pub mod vec {
        use super::*;

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Vec<u8>>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let v: Vec<Cow<'de, str>> = Deserialize::deserialize(deserializer)?;
            Ok(v.into_iter()
                .map(|s| base64::decode(s.as_ref()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(de::Error::custom)?)
        }
    }
}

mod bigint_json {
    use super::*;
    use serde::de;
    use std::borrow::Cow;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<BigInt, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
        Ok(s.parse().map_err(de::Error::custom)?)
    }
}

mod block_messages_json {
    use super::*;
    use serde::de;

    #[derive(Deserialize)]
    struct BlockMessageJson {
        #[serde(with = "address::json")]
        miner_addr: Address,
        win_count: i64,
        #[serde(with = "base64_bytes::vec")]
        messages: Vec<Vec<u8>>,
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

mod message_receipt_vec {
    use super::*;

    #[derive(Deserialize)]
    struct MessageReceiptVector {
        exit_code: ExitCode,
        #[serde(rename = "return", with = "base64_bytes")]
        return_value: Vec<u8>,
        gas_used: i64,
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<MessageReceipt>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Vec<MessageReceiptVector> = Deserialize::deserialize(deserializer)?;
        Ok(s.into_iter()
            .map(|v| MessageReceipt {
                exit_code: v.exit_code,
                return_data: Serialized::new(v.return_value),
                gas_used: v.gas_used,
            })
            .collect())
    }
}

#[derive(Debug, Deserialize)]
struct StateTreeVector {
    #[serde(with = "cid::json")]
    root_cid: Cid,
}

#[derive(Debug, Deserialize)]
struct GenerationData {
    #[serde(default)]
    source: String,
    #[serde(default)]
    version: String,
}

#[derive(Debug, Deserialize)]
struct MetaData {
    id: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    comment: String,
    gen: Vec<GenerationData>,
}

#[derive(Debug, Deserialize)]
struct PreConditions {
    epoch: ChainEpoch,
    state_tree: StateTreeVector,
}

#[derive(Debug, Deserialize)]
struct PostConditions {
    state_tree: StateTreeVector,
    #[serde(with = "message_receipt_vec")]
    receipts: Vec<MessageReceipt>,
    #[serde(default, with = "cid::json::vec")]
    receipts_roots: Vec<Cid>,
}

#[derive(Debug, Deserialize)]
struct MessageVector {
    #[serde(with = "base64_bytes")]
    bytes: Vec<u8>,
    #[serde(default)]
    epoch: Option<ChainEpoch>,
}

#[derive(Debug, Deserialize)]
struct TipsetVector {
    epoch: ChainEpoch,
    #[serde(with = "bigint_json")]
    basefee: BigInt,
    #[serde(with = "block_messages_json")]
    blocks: Vec<BlockMessages>,
}

#[derive(Debug, Deserialize)]
struct Selector {
    #[serde(default)]
    puppet_actor: Option<String>,
    #[serde(default)]
    chaos_actor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "class")]
enum TestVector {
    #[serde(rename = "message")]
    Message {
        selector: Option<Selector>,
        #[serde(rename = "_meta")]
        meta: Option<MetaData>,

        #[serde(with = "base64_bytes")]
        car: Vec<u8>,
        preconditions: PreConditions,
        apply_messages: Vec<MessageVector>,
        postconditions: PostConditions,
    },
    #[serde(rename = "block")]
    Block {
        selector: Option<Selector>,
        #[serde(rename = "_meta")]
        meta: Option<MetaData>,
    },
    #[serde(rename = "tipset")]
    Tipset {
        selector: Option<Selector>,
        #[serde(rename = "_meta")]
        meta: Option<MetaData>,

        #[serde(with = "base64_bytes")]
        car: Vec<u8>,
        preconditions: PreConditions,
        apply_tipsets: Vec<TipsetVector>,
        postconditions: PostConditions,
    },
    #[serde(rename = "chain")]
    Chain {
        selector: Option<Selector>,
        #[serde(rename = "_meta")]
        meta: Option<MetaData>,
    },
}

fn is_valid_file(entry: &DirEntry) -> bool {
    let file_name = match entry.path().to_str() {
        Some(file) => file,
        None => return false,
    };
    for rx in SKIP_TESTS.iter() {
        if rx.is_match(file_name) {
            return false;
        }
    }
    file_name.ends_with(".json")
}

struct TestRand;
impl Rand for TestRand {
    fn get_chain_randomness<DB: BlockStore>(
        &self,
        _: &DB,
        _: DomainSeparationTag,
        _: ChainEpoch,
        _: &[u8],
    ) -> Result<[u8; 32], Box<dyn StdError>> {
        Ok(*b"i_am_random_____i_am_random_____")
    }
    fn get_beacon_randomness<DB: BlockStore>(
        &self,
        _: &DB,
        _: DomainSeparationTag,
        _: ChainEpoch,
        _: &[u8],
    ) -> Result<[u8; 32], Box<dyn StdError>> {
        Ok(*b"i_am_random_____i_am_random_____")
    }
}

struct TestSyscalls;
impl Syscalls for TestSyscalls {
    fn verify_signature(
        &self,
        _: &Signature,
        _: &Address,
        _: &[u8],
    ) -> Result<(), Box<dyn StdError>> {
        Ok(())
    }
    fn verify_seal(&self, _: &SealVerifyInfo) -> Result<(), Box<dyn StdError>> {
        Ok(())
    }
    fn verify_post(&self, _: &WindowPoStVerifyInfo) -> Result<(), Box<dyn StdError>> {
        Ok(())
    }

    // TODO check if this should be defaulted as well
    fn verify_consensus_fault(
        &self,
        _: &[u8],
        _: &[u8],
        _: &[u8],
    ) -> Result<Option<ConsensusFault>, Box<dyn StdError>> {
        Ok(None)
    }
}

fn load_car(gzip_bz: &[u8]) -> Result<db::MemoryDB, Box<dyn StdError>> {
    let bs = db::MemoryDB::default();

    // Decode gzip bytes
    let d = GzDecoder::new(gzip_bz);

    // Load car file with bytes
    forest_car::load_car(&bs, d)?;
    Ok(bs)
}

fn check_msg_result(
    expected_rec: &MessageReceipt,
    actual_rec: &MessageReceipt,
    label: impl fmt::Display,
) -> Result<(), String> {
    let (expected, actual) = (expected_rec.exit_code, actual_rec.exit_code);
    if expected != actual {
        return Err(format!(
            "exit code of msg {} did not match; expected: {:?}, got {:?}",
            label, expected, actual
        ));
    }

    let (expected, actual) = (expected_rec.gas_used, actual_rec.gas_used);
    if expected != actual {
        return Err(format!(
            "gas used of msg {} did not match; expected: {}, got {}",
            label, expected, actual
        ));
    }

    let (expected, actual) = (&expected_rec.return_data, &actual_rec.return_data);
    if expected != actual {
        return Err(format!(
            "return data of msg {} did not match; expected: {}, got {}",
            label,
            base64::encode(expected.as_slice()),
            base64::encode(actual.as_slice())
        ));
    }

    Ok(())
}

fn execute_message(
    bs: &db::MemoryDB,
    msg: &UnsignedMessage,
    pre_root: &Cid,
    epoch: ChainEpoch,
    selector: &Option<Selector>,
) -> Result<(ApplyRet, Cid), Box<dyn StdError>> {
    let mut vm = VM::<_, _, _>::new(
        pre_root,
        bs,
        epoch,
        TestSyscalls,
        &TestRand,
        BASE_FEE.clone(),
    )?;

    if let Some(s) = &selector {
        if s.puppet_actor
            .as_ref()
            .map(|s| s == "true")
            .unwrap_or_default()
        {
            vm.register_actor(PUPPET_ACTOR_CODE_ID.clone());
        }
        if s.chaos_actor
            .as_ref()
            .map(|s| s == "true")
            .unwrap_or_default()
        {
            vm.register_actor(CHAOS_ACTOR_CODE_ID.clone());
        }
    }

    let ret = vm.apply_message(msg)?;

    let root = vm.flush()?;
    Ok((ret, root))
}

fn execute_message_vector(
    selector: Option<Selector>,
    car: Vec<u8>,
    preconditions: PreConditions,
    apply_messages: Vec<MessageVector>,
    postconditions: PostConditions,
) -> Result<(), Box<dyn StdError>> {
    let bs = load_car(car.as_slice())?;

    let mut epoch = preconditions.epoch;
    let mut root = preconditions.state_tree.root_cid;

    for (i, m) in apply_messages.iter().enumerate() {
        let msg = UnsignedMessage::unmarshal_cbor(&m.bytes)?;

        if let Some(ep) = m.epoch {
            epoch = ep;
        }

        let (ret, post_root) = execute_message(&bs, &msg, &root, epoch, &selector)?;
        root = post_root;

        let receipt = &postconditions.receipts[i];
        check_msg_result(receipt, &ret.msg_receipt, i)?;
    }

    if root != postconditions.state_tree.root_cid {
        return Err(format!(
            "wrong post root cid; expected {}, but got {}",
            postconditions.state_tree.root_cid, root
        )
        .into());
    }

    Ok(())
}

struct ExecuteTipsetResult {
    receipts_root: Cid,
    post_state_root: Cid,
    _applied_messages: Vec<UnsignedMessage>,
    applied_results: Vec<ApplyRet>,
}
fn execute_tipset(
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

fn execute_tipset_vector(
    _selector: Option<Selector>,
    car: Vec<u8>,
    preconditions: PreConditions,
    tipsets: Vec<TipsetVector>,
    postconditions: PostConditions,
) -> Result<(), Box<dyn StdError>> {
    let bs = Arc::new(load_car(car.as_slice())?);

    let mut prev_epoch = preconditions.epoch;
    let mut root = preconditions.state_tree.root_cid;

    let mut receipt_idx = 0;
    for (i, ts) in tipsets.into_iter().enumerate() {
        let ExecuteTipsetResult {
            receipts_root,
            post_state_root,
            applied_results,
            ..
        } = execute_tipset(Arc::clone(&bs), &root, prev_epoch, &ts)?;

        for (j, v) in applied_results.into_iter().enumerate() {
            check_msg_result(
                &postconditions.receipts[receipt_idx],
                &v.msg_receipt,
                format!("{} of tipset {}", j, i),
            )?;
            receipt_idx += 1;
        }

        // Compare receipts root
        let (expected, actual) = (&postconditions.receipts_roots[i], &receipts_root);
        if expected != actual {
            return Err(format!(
                "post receipts did not match; expected: {:?}, got {:?}",
                expected, actual
            )
            .into());
        }

        prev_epoch = ts.epoch;
        root = post_state_root;
    }

    if root != postconditions.state_tree.root_cid {
        return Err(format!(
            "wrong post root cid; expected {}, but got {}",
            postconditions.state_tree.root_cid, root
        )
        .into());
    }

    Ok(())
}

#[test]
fn conformance_test_runner() {
    let walker = WalkDir::new("test-vectors/corpus").into_iter();
    let mut failed = Vec::new();
    let mut succeeded = 0;
    for entry in walker.filter_map(|e| e.ok()).filter(is_valid_file) {
        let file = File::open(entry.path()).unwrap();
        let reader = BufReader::new(file);
        let vector: TestVector = serde_json::from_reader(reader).unwrap();
        let test_name = entry.path().display();

        match vector {
            TestVector::Message {
                selector,
                meta,
                car,
                preconditions,
                apply_messages,
                postconditions,
            } => {
                if let Err(e) = execute_message_vector(
                    selector,
                    car,
                    preconditions,
                    apply_messages,
                    postconditions,
                ) {
                    failed.push((test_name.to_string(), meta, e));
                } else {
                    println!("{} succeeded", test_name);
                    succeeded += 1;
                }
            }
            TestVector::Tipset {
                selector,
                meta,
                car,
                preconditions,
                apply_tipsets,
                postconditions,
            } => {
                if let Err(e) = execute_tipset_vector(
                    selector,
                    car,
                    preconditions,
                    apply_tipsets,
                    postconditions,
                ) {
                    failed.push((test_name.to_string(), meta, e));
                } else {
                    println!("{} succeeded", test_name);
                    succeeded += 1;
                }
            }
            _ => panic!("Unsupported test vector class"),
        }
    }

    println!("{}/{} tests passed:", succeeded, failed.len() + succeeded);
    if !failed.is_empty() {
        for (path, meta, e) in failed {
            eprintln!(
                "file {} failed:\n\tMeta: {:?}\n\tError: {}\n",
                path, meta, e
            );
        }
        panic!()
    }
}

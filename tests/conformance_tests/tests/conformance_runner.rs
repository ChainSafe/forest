// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "submodule_tests")]

use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use crypto::{DomainSeparationTag, Signature};
use encoding::Cbor;
use fil_types::{SealVerifyInfo, WindowPoStVerifyInfo};
use flate2::read::GzDecoder;
use forest_message::{MessageReceipt, UnsignedMessage};
use interpreter::{ApplyRet, Rand, VM};
use runtime::{ConsensusFault, Syscalls};
use serde::{Deserialize, Deserializer};
use std::error::Error as StdError;
use std::fs::File;
use std::io::{BufReader, Read};
use vm::{ExitCode, Serialized};
use walkdir::{DirEntry, WalkDir};

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
}

#[derive(Debug, Deserialize)]
struct MessageVector {
    #[serde(with = "base64_bytes")]
    bytes: Vec<u8>,
    #[serde(default)]
    epoch: Option<ChainEpoch>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "class")]
enum TestVector {
    #[serde(rename = "message")]
    Message {
        selector: Option<String>,
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
        selector: Option<String>,
        #[serde(rename = "_meta")]
        meta: Option<MetaData>,
    },
    #[serde(rename = "tipset")]
    Tipset {
        selector: Option<String>,
        #[serde(rename = "_meta")]
        meta: Option<MetaData>,
    },
    #[serde(rename = "chain")]
    Chain {
        selector: Option<String>,
        #[serde(rename = "_meta")]
        meta: Option<MetaData>,
    },
}

fn is_test_file(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.ends_with(".json"))
        .unwrap_or(false)
}

struct TestRand;
impl Rand for TestRand {
    fn get_randomness<DB: blockstore::BlockStore>(
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

fn execute_message(
    msg: &UnsignedMessage,
    pre_root: &Cid,
    bs: &db::MemoryDB,
    epoch: ChainEpoch,
) -> Result<(ApplyRet, Cid), Box<dyn StdError>> {
    let mut vm = VM::<_, _, _>::new(pre_root, bs, epoch, TestSyscalls, &TestRand)?;

    // TODO register puppet actor (and conditionally chaos actor)

    let ret = vm.apply_message(msg)?;

    let root = vm.flush()?;
    Ok((ret, root))
}

fn execute_message_vector(
    _selector: Option<String>,
    car: Vec<u8>,
    preconditions: PreConditions,
    apply_messages: Vec<MessageVector>,
    postconditions: PostConditions,
) -> Result<(), Box<dyn StdError>> {
    let bs = db::MemoryDB::default();

    let mut epoch = preconditions.epoch;
    let mut root = preconditions.state_tree.root_cid;

    // Decode gzip bytes
    let mut d = GzDecoder::new(car.as_slice());
    let mut decoded = Vec::new();
    d.read_to_end(&mut decoded)?;

    // Load car file with bytes
    let reader = BufReader::new(decoded.as_slice());
    forest_car::load_car(&bs, reader)?;

    for (i, m) in apply_messages.iter().enumerate() {
        let msg = UnsignedMessage::unmarshal_cbor(&m.bytes)?;

        if let Some(ep) = m.epoch {
            epoch = ep;
        }

        let (ret, post_root) = execute_message(&msg, &root, &bs, epoch)?;
        root = post_root;

        let receipt = &postconditions.receipts[i];
        let (expected, actual) = (receipt.exit_code, ret.msg_receipt.exit_code);
        if expected != actual {
            return Err(format!(
                "exit code of msg {} did not match; expected: {:?}, got {:?}",
                i, expected, actual
            )
            .into());
        }

        let (expected, actual) = (receipt.gas_used, ret.msg_receipt.gas_used);
        if expected != actual {
            return Err(format!(
                "gas used of msg {} did not match; expected: {}, got {}",
                i, expected, actual
            )
            .into());
        }
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
// TODO remove ignore when blocking changes come in
#[ignore]
fn conformance_test_runner() {
    let walker = WalkDir::new("test-vectors/corpus").into_iter();
    for entry in walker.filter_map(|e| e.ok()).filter(is_test_file) {
        println!("{}", entry.path().display());
        let file = File::open(entry.path()).unwrap();
        let reader = BufReader::new(file);
        let vector: TestVector = serde_json::from_reader(reader).unwrap();
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
                    panic!(
                        "Message vector failed:\n\tMeta: {:?}\n\tError: {}\n",
                        meta, e
                    );
                }
            }
            _ => panic!("Unsupported test vector class"),
        }
    }
}

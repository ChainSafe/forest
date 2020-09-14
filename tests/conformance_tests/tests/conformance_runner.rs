// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "submodule_tests")]

#[macro_use]
extern crate lazy_static;

use conformance_tests::*;
use encoding::Cbor;
use flate2::read::GzDecoder;
use forest_message::{MessageReceipt, UnsignedMessage};
use regex::Regex;
use std::error::Error as StdError;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
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
        // TODO This fails but is blocked on miner actor refactor, remove skip after that comes in
        Regex::new(r"test-vectors/corpus/reward/reward--ok-miners-awarded-no-premiums.json").unwrap(),
    ];
}

fn is_valid_file(entry: &DirEntry) -> bool {
    let file_name = match entry.path().to_str() {
        Some(file) => file,
        None => return false,
    };
    for rx in SKIP_TESTS.iter() {
        if rx.is_match(file_name) {
            println!("SKIPPING: {}", file_name);
            return false;
        }
    }
    file_name.ends_with(".json")
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
    pretty_env_logger::init();
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

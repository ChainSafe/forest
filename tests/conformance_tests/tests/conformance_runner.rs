// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "submodule_tests")]

#[macro_use]
extern crate lazy_static;

use chain::ChainStore;
use cid::Cid;
use clock::ChainEpoch;
use conformance_tests::*;
use encoding::Cbor;
use fil_types::TOTAL_FILECOIN;
use flate2::read::GzDecoder;
use forest_message::{MessageReceipt, UnsignedMessage};
use futures::AsyncRead;
use interpreter::ApplyRet;
use num_bigint::{BigInt, ToBigInt};
use paramfetch::{get_params_default, SectorSizeOpt};
use regex::Regex;
use state_manager::StateManager;
use statediff::print_state_diff;
use std::error::Error as StdError;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use walkdir::{DirEntry, WalkDir};

lazy_static! {
    static ref DEFAULT_BASE_FEE: BigInt = BigInt::from(100);
    static ref SKIP_TESTS: Vec<Regex> = vec![
        // No reason for this, Lotus specific test
        Regex::new(r"x--actor_abort--negative-exit-code").unwrap(),

        // Our VM doesn't handle panics
        Regex::new(r"x--actor_abort--no-exit-code").unwrap(),

        // These 2 tests ignore test cases for Chaos actor that are checked at compile time
        Regex::new(r"test-vectors/corpus/vm_violations/x--state_mutation--after-transaction").unwrap(),
        Regex::new(r"test-vectors/corpus/vm_violations/x--state_mutation--readonly").unwrap(),
    ];
}

fn is_valid_file(entry: &DirEntry) -> bool {
    let file_name = match entry.path().to_str() {
        Some(file) => file,
        None => return false,
    };

    if let Ok(s) = ::std::env::var("FOREST_CONF") {
        return file_name == s;
    }

    for rx in SKIP_TESTS.iter() {
        if rx.is_match(file_name) {
            println!("SKIPPING: {}", file_name);
            return false;
        }
    }
    file_name.ends_with(".json")
}

struct GzipDecoder<R>(GzDecoder<R>);

impl<R: std::io::Read + Unpin> AsyncRead for GzipDecoder<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        Poll::Ready(std::io::Read::read(&mut self.0, buf))
    }
}

async fn load_car(gzip_bz: &[u8]) -> Result<db::MemoryDB, Box<dyn StdError>> {
    let bs = db::MemoryDB::default();

    // Decode gzip bytes
    let d = GzipDecoder(GzDecoder::new(gzip_bz));

    // Load car file with bytes
    forest_car::load_car(&bs, d).await?;
    Ok(bs)
}

fn check_msg_result(
    expected_rec: &MessageReceipt,
    ret: &ApplyRet,
    label: impl fmt::Display,
) -> Result<(), String> {
    let error = ret.act_error.as_ref().map(|e| e.msg());
    let actual_rec = &ret.msg_receipt;
    let (expected, actual) = (expected_rec.exit_code, actual_rec.exit_code);
    if expected != actual {
        return Err(format!(
            "exit code of msg {} did not match; expected: {:?}, got {:?}. Error: {}",
            label,
            expected,
            actual,
            error.unwrap_or("No error reported with exit code")
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

fn compare_state_roots(bs: &db::MemoryDB, root: &Cid, expected_root: &Cid) -> Result<(), String> {
    if root != expected_root {
        let error_msg = format!(
            "wrong post root cid; expected {}, but got {}",
            expected_root, root
        );

        if std::env::var("FOREST_DIFF") == Ok("1".to_owned()) {
            println!("{}:", error_msg);

            print_state_diff(bs, root, expected_root, None).unwrap();
        }

        return Err(error_msg.into());
    }
    Ok(())
}

async fn execute_message_vector(
    selector: &Option<Selector>,
    car: &[u8],
    root_cid: Cid,
    base_fee: Option<f64>,
    circ_supply: Option<f64>,
    apply_messages: &[MessageVector],
    postconditions: &PostConditions,
    randomness: &Randomness,
    variant: &Variant,
) -> Result<(), Box<dyn StdError>> {
    let bs = load_car(car).await?;

    let mut base_epoch: ChainEpoch = variant.epoch;
    let mut root = root_cid;

    for (i, m) in apply_messages.iter().enumerate() {
        let msg = UnsignedMessage::unmarshal_cbor(&m.bytes)?;

        if let Some(ep) = m.epoch_offset {
            base_epoch += ep;
        }

        let (ret, post_root) = execute_message(
            &bs,
            &selector,
            ExecuteMessageParams {
                pre_root: &root,
                epoch: base_epoch,
                msg: &to_chain_msg(msg),
                circ_supply: circ_supply
                    .map(|i| i.to_bigint().unwrap())
                    .unwrap_or(TOTAL_FILECOIN.clone()),
                basefee: base_fee
                    .map(|i| i.to_bigint().unwrap())
                    .unwrap_or(DEFAULT_BASE_FEE.clone()),
                randomness: ReplayingRand::new(randomness),
            },
        )?;
        root = post_root;

        let receipt = &postconditions.receipts[i];
        check_msg_result(receipt, &ret, i)?;
    }

    compare_state_roots(&bs, &root, &postconditions.state_tree.root_cid)?;

    Ok(())
}

async fn execute_tipset_vector(
    _selector: &Option<Selector>,
    car: &[u8],
    root_cid: Cid,
    tipsets: &[TipsetVector],
    postconditions: &PostConditions,
    randomness: &Randomness,
    variant: &Variant,
) -> Result<(), Box<dyn StdError>> {
    let bs = load_car(car).await?;
    let bs = Arc::new(bs);
    let sm = Arc::new(StateManager::new(Arc::new(ChainStore::new(bs))));
    genesis::initialize_genesis(None, &sm).await.unwrap();

    let base_epoch = variant.epoch;
    let mut root = root_cid;

    let mut receipt_idx = 0;
    let mut parent_epoch = base_epoch;
    for (i, ts) in tipsets.into_iter().enumerate() {
        let exec_epoch = base_epoch + ts.epoch_offset;
        let ExecuteTipsetResult {
            receipts_root,
            post_state_root,
            applied_results,
            ..
        } = execute_tipset(
            Arc::clone(&sm),
            ExecuteTipsetParams {
                pre_root: &root,
                parent_epoch,
                tipset: &ts,
                exec_epoch,
                randomness: ReplayingRand::new(randomness),
            },
        )?;

        for (j, apply_ret) in applied_results.into_iter().enumerate() {
            check_msg_result(
                &postconditions.receipts[receipt_idx],
                &apply_ret,
                format!("{} of tipset {}", j, i),
            )?;
            receipt_idx += 1;
        }

        // Compare receipts root
        let (expected, actual) = (&postconditions.receipts_roots[i], &receipts_root);
        compare_state_roots(sm.blockstore(), &expected, &actual)?;

        if expected != actual {
            return Err(format!(
                "post receipts did not match; expected: {:?}, got {:?}",
                expected, actual
            )
            .into());
        }

        parent_epoch = exec_epoch;
        root = post_state_root;
    }

    compare_state_roots(sm.blockstore(), &root, &postconditions.state_tree.root_cid)?;

    Ok(())
}

#[async_std::test]
async fn conformance_test_runner() {
    pretty_env_logger::init();

    // Retrieve verification params
    get_params_default(SectorSizeOpt::Keys, false)
        .await
        .unwrap();

    let walker = WalkDir::new("test-vectors/corpus").into_iter();
    let mut failed = Vec::new();
    let mut succeeded = 0;
    for entry in walker.filter_map(|e| e.ok()).filter(is_valid_file) {
        let file = File::open(entry.path()).unwrap();
        let reader = BufReader::new(file);
        let test_name = entry.path().display();
        let vector: TestVector = serde_json::from_reader(reader).unwrap();

        match vector {
            TestVector::Message {
                selector,
                meta,
                car,
                preconditions,
                apply_messages,
                postconditions,
                randomness,
            } => {
                for variant in preconditions.variants {
                    if let Err(e) = execute_message_vector(
                        &selector,
                        &car,
                        preconditions.state_tree.root_cid.clone(),
                        preconditions.basefee,
                        preconditions.circ_supply,
                        &apply_messages,
                        &postconditions,
                        &randomness,
                        &variant,
                    )
                    .await
                    {
                        failed.push((
                            format!("{} variant {}", test_name, variant.id),
                            meta.clone(),
                            e,
                        ));
                    } else {
                        println!("{} succeeded", test_name);
                        succeeded += 1;
                    }
                }
            }
            TestVector::Tipset {
                selector,
                meta,
                car,
                preconditions,
                apply_tipsets,
                postconditions,
                randomness,
            } => {
                for variant in preconditions.variants {
                    if let Err(e) = execute_tipset_vector(
                        &selector,
                        &car,
                        preconditions.state_tree.root_cid.clone(),
                        &apply_tipsets,
                        &postconditions,
                        &randomness,
                        &variant,
                    )
                    .await
                    {
                        failed.push((
                            format!("{} variant {}", test_name, variant.id),
                            meta.clone(),
                            e,
                        ));
                    } else {
                        println!("{} succeeded", test_name);
                        succeeded += 1;
                    }
                }
            }
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

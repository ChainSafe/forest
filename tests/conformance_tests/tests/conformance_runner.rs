// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "submodule_tests")]

#[macro_use]
extern crate lazy_static;

use address::Address;
use blockstore::resolve::resolve_cids_recursive;
use cid::{json::CidJson, Cid};
use clock::ChainEpoch;
use colored::*;
use conformance_tests::*;
use difference::{Changeset, Difference};
use encoding::Cbor;
use fil_types::{HAMT_BIT_WIDTH, TOTAL_FILECOIN};
use flate2::read::GzDecoder;
use forest_message::{MessageReceipt, UnsignedMessage};
use interpreter::ApplyRet;
use ipld::json::{IpldJson, IpldJsonRef};
use ipld::Ipld;
use ipld_hamt::{BytesKey, Hamt};
use num_bigint::{BigInt, ToBigInt};
use paramfetch::{get_params_default, SectorSizeOpt};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use vm::ActorState;
use walkdir::{DirEntry, WalkDir};

lazy_static! {
    static ref DEFAULT_BASE_FEE: BigInt = BigInt::from(100);
    static ref SKIP_TESTS: Vec<Regex> = vec![
        Regex::new(r"test-vectors/corpus/vm_violations/x--").unwrap(),
        Regex::new(r"test-vectors/corpus/nested/x--").unwrap(),
        // These tests are marked as invalid as they return wrong exit code on Lotus
        Regex::new(r"actor_creation/x--params*").unwrap(),

        // These 2 tests ignore test cases for Chaos actor that are checked at compile time
        Regex::new(r"test-vectors/corpus/vm_violations/x--state_mutation--after-transaction.json").unwrap(),
        Regex::new(r"test-vectors/corpus/vm_violations/x--state_mutation--readonly.json").unwrap(),

        // Same as marked tests above -- Go impl has the incorrect behaviour
        Regex::new(r"fil_1_storageminer-SubmitWindowedPoSt-SysErrSenderInvalid-").unwrap(),
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

#[derive(Serialize, Deserialize)]
struct ActorStateResolved {
    code: CidJson,
    sequence: u64,
    balance: String,
    state: IpldJson,
}

fn root_to_state_map(
    bs: &db::MemoryDB,
    root: &Cid,
) -> Result<BTreeMap<String, ActorStateResolved>, Box<dyn StdError>> {
    let mut actors = BTreeMap::new();
    let hamt: Hamt<_, _> = Hamt::load_with_bit_width(root, bs, HAMT_BIT_WIDTH)?;
    hamt.for_each(|k: &BytesKey, actor: &ActorState| {
        let addr = Address::from_bytes(&k.0)?;

        let resolved =
            resolve_cids_recursive(bs, &actor.state).unwrap_or(Ipld::Link(actor.state.clone()));
        let resolved_state = ActorStateResolved {
            state: IpldJson(resolved),
            code: CidJson(actor.code.clone()),
            balance: actor.balance.to_string(),
            sequence: actor.sequence,
        };

        actors.insert(addr.to_string(), resolved_state);
        Ok(())
    })
    .unwrap();

    Ok(actors)
}

/// Tries to resolve state tree actors, if all data exists in store.
/// The actors hamt is hard to parse in a diff, so this attempts to remedy this.
fn try_resolve_actor_states(
    bs: &db::MemoryDB,
    root: &Cid,
    expected_root: &Cid,
) -> Result<Changeset, Box<dyn StdError>> {
    let e_state = root_to_state_map(bs, expected_root)?;
    let c_state = root_to_state_map(bs, root)?;

    let expected_json = serde_json::to_string_pretty(&e_state)?;
    let actual_json = serde_json::to_string_pretty(&c_state)?;

    Ok(Changeset::new(&expected_json, &actual_json, "\n"))
}

fn compare_state_roots(bs: &db::MemoryDB, root: &Cid, expected_root: &Cid) -> Result<(), String> {
    if root != expected_root {
        let error_msg = format!(
            "wrong post root cid; expected {}, but got {}",
            expected_root, root
        );

        if std::env::var("FOREST_DIFF") == Ok("1".to_owned()) {
            let Changeset { diffs, .. } = try_resolve_actor_states(bs, root, expected_root)
                .unwrap_or_else(|e| {
                    println!(
                        "Could not resolve actor states: {}\nUsing default resolution:",
                        e
                    );
                    let expected = resolve_cids_recursive(bs, &expected_root)
                        .expect("Failed to populate Ipld");
                    let actual =
                        resolve_cids_recursive(bs, &root).expect("Failed to populate Ipld");

                    let expected_json =
                        serde_json::to_string_pretty(&IpldJsonRef(&expected)).unwrap();
                    let actual_json = serde_json::to_string_pretty(&IpldJsonRef(&actual)).unwrap();

                    Changeset::new(&expected_json, &actual_json, "\n")
                });

            println!("{}:", error_msg);

            for diff in diffs.iter() {
                match diff {
                    Difference::Same(x) => {
                        println!(" {}", x);
                    }
                    Difference::Add(x) => {
                        println!("{}", format!("+{}", x).green());
                    }
                    Difference::Rem(x) => {
                        println!("{}", format!("-{}", x).red());
                    }
                }
            }
        }

        return Err(error_msg.into());
    }
    Ok(())
}

fn execute_message_vector(
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
    let bs = load_car(car)?;

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

fn execute_tipset_vector(
    _selector: &Option<Selector>,
    car: &[u8],
    root_cid: Cid,
    tipsets: &[TipsetVector],
    postconditions: &PostConditions,
    variant: &Variant,
) -> Result<(), Box<dyn StdError>> {
    let bs = Arc::new(load_car(car)?);

    let base_epoch = variant.epoch;
    let mut root = root_cid;

    let mut receipt_idx = 0;
    let mut prev_epoch = base_epoch;
    for (i, ts) in tipsets.into_iter().enumerate() {
        let exec_epoch = base_epoch + ts.epoch_offset;
        let ExecuteTipsetResult {
            receipts_root,
            post_state_root,
            applied_results,
            ..
        } = execute_tipset(Arc::clone(&bs), &root, prev_epoch, &ts, exec_epoch)?;

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
        if expected != actual {
            return Err(format!(
                "post receipts did not match; expected: {:?}, got {:?}",
                expected, actual
            )
            .into());
        }

        prev_epoch = exec_epoch;
        root = post_state_root;
    }

    compare_state_roots(bs.as_ref(), &root, &postconditions.state_tree.root_cid)?;

    Ok(())
}

#[test]
fn conformance_test_runner() {
    pretty_env_logger::init();

    // Retrieve verification params
    async_std::task::block_on(get_params_default(SectorSizeOpt::Keys, false)).unwrap();

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
                    if variant.nv > 3 {
                        // Skip v2 upgrade and above
                        continue;
                    }
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
                    ) {
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
            } => {
                for variant in preconditions.variants {
                    if variant.nv > 3 {
                        // Skip v2 upgrade and above
                        continue;
                    }
                    if let Err(e) = execute_tipset_vector(
                        &selector,
                        &car,
                        preconditions.state_tree.root_cid.clone(),
                        &apply_tipsets,
                        &postconditions,
                        &variant,
                    ) {
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

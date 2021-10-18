// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "submodule_tests")]
use actor::actorv2::CHAOS_ACTOR_CODE_ID;
use chain::ChainStore;
use cid::Cid;
use encoding::Cbor;
use fil_types::TOTAL_FILECOIN;
use flate2::read::GzDecoder;
use forest_message::{MessageReceipt, UnsignedMessage};
use futures::AsyncRead;
use interpreter::ApplyRet;
use interpreter::VM;
use lazy_static::lazy_static;
use networks::get_network_version_default;
use num_bigint::{BigInt, ToBigInt};
use paramfetch::{get_params_default, SectorSizeOpt};
use regex::Regex;
use specs_actors::*;
use state_manager::StateManager;
use statediff::print_state_diff;
use walkdir::{DirEntry, WalkDir};

use std::error::Error as StdError;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::rand_replay::ReplayingRand;
use crate::{MockCircSupply, MockStateLB};

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

fn is_valid_file(entry: &DirEntry) -> bool {
    let file_name = match entry.path().to_str() {
        Some(file) => file,
        None => return false,
    };

    if let Ok(s) = ::std::env::var("FOREST_CONF") {
        return file_name == s;
    }

    file_name.ends_with(".json")
}

fn execute_message(
    block_store: &db::MemoryDB,
    params: ExecuteMessageParams,
) -> Result<(ApplyRet, Cid), Box<dyn StdError>> {
    let circ_supply = MockCircSupply(params.circ_supply);
    let lb = MockStateLB(block_store);
    let mut vm = VM::<_, _, _, _, _>::new(
        params.pre_root,
        block_store,
        params.epoch,
        &params.randomness,
        params.basefee,
        get_network_version_default,
        &circ_supply,
        &lb,
    )?;

    let ret = vm.apply_message(params.msg)?;
    let root = vm.flush()?;
    Ok((ret, root))
}

async fn load_car(gzip_bz: &[u8]) -> Result<db::MemoryDB, Box<dyn StdError>> {
    let block_store = db::MemoryDB::default();

    // Decode gzip bytes
    let d = GzipDecoder(GzDecoder::new(gzip_bz));

    // Load CAR file with bytes
    forest_car::load_car(&block_store, d).await?;
    Ok(block_store)
}

fn check_msg_result(
    expected_receipt: &MessageReceipt,
    ret: &ApplyRet,
    label: impl fmt::Display,
) -> Result<(), String> {
    let error = ret.act_error.as_ref().map(|e| e.msg());
    let actual_rec = &ret.msg_receipt;
    let (expected, actual) = (expected_receipt.exit_code, actual_rec.exit_code);
    if expected != actual {
        return Err(format!(
            "exit code of msg {} did not match; expected: {:?}, got {:?}. Error: {}",
            label,
            expected,
            actual,
            error.unwrap_or("No error reported with exit code")
        ));
    }

    let (expected, actual) = (expected_receipt.gas_used, actual_rec.gas_used);
    if expected != actual {
        return Err(format!(
            "gas used of msg {} did not match; expected: {}, got {}",
            label, expected, actual
        ));
    }

    let (expected, actual) = (&expected_receipt.return_data, &actual_rec.return_data);
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

async fn execute_message_vector(
    car: &[u8],
    root_cid: Cid,
    base_fee: Option<f64>,
    circ_supply: Option<f64>,
    apply_messages: &[ApplyMessage],
    postconditions: &PostConditions,
    randomness: &Randomness,
    variant: &Variant,
) -> Result<(), Box<dyn StdError>> {
    let block_store = load_car(car).await?;
    let bs = Arc::new(block_store);
    let state_manager = Arc::new(StateManager::new(Arc::new(ChainStore::new(bs.clone()))));
    genesis::initialize_genesis(None, &state_manager)
        .await
        .expect("Initializing genesis must succeed in order to run the test");

    let base_epoch = variant.epoch;
    let mut root = root_cid;

    for (i, msg) in apply_messages.iter().enumerate() {
        let unsigned_msg = UnsignedMessage::unmarshal_cbor(&msg.bytes)?;
        let (ret, post_root) = execute_message(
            &bs,
            ExecuteMessageParams {
                pre_root: &root,
                epoch: base_epoch,
                msg: &to_chain_msg(unsigned_msg),
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

fn compare_state_roots(
    block_store: &db::MemoryDB,
    root: &Cid,
    expected_root: &Cid,
) -> Result<(), String> {
    if root != expected_root {
        let error_msg = format!(
            "Post root cid must match expected root cid; expected {}, post {}",
            expected_root, root
        );

        if std::env::var("FOREST_DIFF") == Ok("1".to_owned()) {
            println!("{}:", error_msg);
            print_state_diff(block_store, root, expected_root, None).unwrap();
        }
        return Err(error_msg.into());
    }
    Ok(())
}

#[async_std::test]
async fn specs_actors_test_runner() {
    pretty_env_logger::init();

    get_params_default(SectorSizeOpt::Keys, false)
        .await
        .unwrap();

    let walker = WalkDir::new("specs-actors/test-vectors/determinism").into_iter();
    let mut failed = vec![];
    let mut succeeded = 0;
    for entry in walker.filter_map(|e| e.ok()).filter(is_valid_file) {
        let file = File::open(entry.path()).unwrap();
        let reader = BufReader::new(file);
        let test_name = entry.path().display();
        let vector: TestVector = match serde_json::from_reader(reader) {
            Ok(vector) => vector,
            Err(why) => {
                panic!("Deserializing test [{}] failed: {}", test_name, why);
            }
        };

        for variant in vector.preconditions.variants {
            if let Err(e) = execute_message_vector(
                &vector.car,
                vector.preconditions.state_tree.root_cid,
                vector.preconditions.base_fee,
                vector.preconditions.circ_supply,
                &vector.apply_messages,
                &vector.postconditions,
                &vector.randomness,
                &variant,
            )
            .await
            {
                failed.push((
                    format!("{} variant {}", test_name, vector.meta.id),
                    vector.meta.clone(),
                    e,
                ));
            } else {
                succeeded += 1;
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

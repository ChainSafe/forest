// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "submodule_tests")]

#[macro_use]
extern crate lazy_static;

use address::Protocol;
use conformance_tests::*;
use crypto::Signature;
use encoding::Cbor;
use flate2::read::GzDecoder;
use forest_message::{ChainMessage, Message, MessageReceipt, SignedMessage, UnsignedMessage};
use regex::Regex;
use std::error::Error as StdError;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use walkdir::{DirEntry, WalkDir};

lazy_static! {
    static ref SKIP_TESTS: [Regex; 78] = [

        Regex::new(r"test-vectors/corpus/vm_violations/x--*").unwrap(),
        Regex::new(r"test-vectors/corpus/nested/x--*").unwrap(),
        // These tests are marked as invalid as they return wrong exit code on Lotus
        Regex::new(r"actor_creation/x--params*").unwrap(),
        // Following two fail for the same invalid exit code return
        Regex::new(r"nested/nested_sends--fail-missing-params.json").unwrap(),
        Regex::new(r"nested/nested_sends--fail-mismatch-params.json").unwrap(),
        // Lotus client does not fail in inner transaction for insufficient funds
        Regex::new(r"test-vectors/corpus/nested/nested_sends--fail-insufficient-funds-for-transfer-in-inner-send.json").unwrap(),
        // TODO This fails but is blocked on miner actor refactor, remove skip after that comes in
        Regex::new(r"test-vectors/corpus/reward/reward--ok-miners-awarded-no-premiums.json").unwrap(),

        // These 2 tests ignore test cases for Chaos actor that are checked at compile time
        // Link to discussion https://github.com/ChainSafe/forest/pull/696/files
        // Maybe should look at fixing to match exit codes
        Regex::new(r"test-vectors/corpus/vm_violations/x--state_mutation--after-transaction.json").unwrap(),
        Regex::new(r"test-vectors/corpus/vm_violations/x--state_mutation--readonly.json").unwrap(),

        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storagemarket/AddBalance/Ok/ext-0001-fil_1_storagemarket-AddBalance-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/AddLockedFund/Ok/ext-0001-fil_1_storageminer-AddLockedFund-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/ChangeMultiaddrs/Ok/ext-0001-fil_1_storageminer-ChangeMultiaddrs-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/ChangePeerID/Ok/ext-0001-fil_1_storageminer-ChangePeerID-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/ChangeWorkerAddress/Ok/ext-0001-fil_1_storageminer-ChangeWorkerAddress-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/DeclareFaults/16/ext-0001-fil_1_storageminer-DeclareFaults-16-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/DeclareFaultsRecovered/Ok/ext-0001-fil_1_storageminer-DeclareFaultsRecovered-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/PreCommitSector/16/ext-0001-fil_1_storageminer-PreCommitSector-16-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/PreCommitSector/SysErrInsufficientFunds/ext-0001-fil_1_storageminer-PreCommitSector-SysErrInsufficientFunds-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/PreCommitSector/SysErrOutOfGas/ext-0001-fil_1_storageminer-PreCommitSector-SysErrOutOfGas-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/ProveCommitSector/Ok/ext-0001-fil_1_storageminer-ProveCommitSector-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/ProveCommitSector/SysErrInsufficientFunds/ext-0001-fil_1_storageminer-ProveCommitSector-SysErrInsufficientFunds-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/ProveCommitSector/SysErrOutOfGas/ext-0001-fil_1_storageminer-ProveCommitSector-SysErrOutOfGas-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/Send/Ok/ext-0001-fil_1_storageminer-Send-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/Send/SysErrInsufficientFunds/ext-0001-fil_1_storageminer-Send-SysErrInsufficientFunds-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/SubmitWindowedPoSt/16/ext-0001-fil_1_storageminer-SubmitWindowedPoSt-16-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/WithdrawBalance/Ok/ext-0001-fil_1_storageminer-WithdrawBalance-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storagepower/CreateMiner/Ok/extracted-msg-0001-fil_1_storagepower-CreateMiner-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0002-init-actor/fil_1_init/Exec/Ok/ext-0002-fil_1_init-Exec-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0003-sends-to-sysactors/fil_1_reward/Send/Ok/ext-0003-fil_1_reward-Send-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0003-sends-to-sysactors/fil_1_storagemarket/Send/Ok/ext-0003-fil_1_storagemarket-Send-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storagemarket/AddBalance/Ok/extracted-msg-fil_1_storagemarket-AddBalance-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storagemarket/AddBalance/SysErrInsufficientFunds/extracted-msg-fil_1_storagemarket-AddBalance-SysErrInsufficientFunds-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storagemarket/PublishStorageDeals/16/extracted-msg-fil_1_storagemarket-PublishStorageDeals-16-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storagemarket/PublishStorageDeals/SysErrOutOfGas/extracted-msg-fil_1_storagemarket-PublishStorageDeals-SysErrOutOfGas-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/AddLockedFund/19/extracted-msg-fil_1_storageminer-AddLockedFund-19-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/ChangeMultiaddrs/Ok/extracted-msg-fil_1_storageminer-ChangeMultiaddrs-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/ChangePeerID/Ok/extracted-msg-fil_1_storageminer-ChangePeerID-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/ChangeWorkerAddress/Ok/extracted-msg-fil_1_storageminer-ChangeWorkerAddress-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/DeclareFaults/16/extracted-msg-fil_1_storageminer-DeclareFaults-16-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/DeclareFaultsRecovered/Ok/extracted-msg-fil_1_storageminer-DeclareFaultsRecovered-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/DeclareFaultsRecovered/SysErrOutOfGas/extracted-msg-fil_1_storageminer-DeclareFaultsRecovered-SysErrOutOfGas-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/PreCommitSector/16/extracted-msg-fil_1_storageminer-PreCommitSector-16-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/PreCommitSector/17/extracted-msg-fil_1_storageminer-PreCommitSector-17-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/PreCommitSector/18/extracted-msg-fil_1_storageminer-PreCommitSector-18-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/PreCommitSector/19/extracted-msg-fil_1_storageminer-PreCommitSector-19-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/PreCommitSector/Ok/extracted-msg-fil_1_storageminer-PreCommitSector-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/PreCommitSector/SysErrInsufficientFunds/extracted-msg-fil_1_storageminer-PreCommitSector-SysErrInsufficientFunds-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/PreCommitSector/SysErrOutOfGas/extracted-msg-fil_1_storageminer-PreCommitSector-SysErrOutOfGas-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/ProveCommitSector/16/extracted-msg-fil_1_storageminer-ProveCommitSector-16-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/ProveCommitSector/17/extracted-msg-fil_1_storageminer-ProveCommitSector-17-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/ProveCommitSector/18/extracted-msg-fil_1_storageminer-ProveCommitSector-18-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/ProveCommitSector/19/extracted-msg-fil_1_storageminer-ProveCommitSector-19-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/ProveCommitSector/Ok/extracted-msg-fil_1_storageminer-ProveCommitSector-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/ProveCommitSector/SysErrInsufficientFunds/extracted-msg-fil_1_storageminer-ProveCommitSector-SysErrInsufficientFunds-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/ProveCommitSector/SysErrOutOfGas/extracted-msg-fil_1_storageminer-ProveCommitSector-SysErrOutOfGas-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/Send/Ok/extracted-msg-fil_1_storageminer-Send-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/Send/SysErrInsufficientFunds/extracted-msg-fil_1_storageminer-Send-SysErrInsufficientFunds-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/SubmitWindowedPoSt/16/extracted-msg-fil_1_storageminer-SubmitWindowedPoSt-16-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/SubmitWindowedPoSt/SysErrSenderInvalid/extracted-msg-fil_1_storageminer-SubmitWindowedPoSt-SysErrSenderInvalid-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/WithdrawBalance/Ok/extracted-msg-fil_1_storageminer-WithdrawBalance-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/WithdrawBalance/SysErrOutOfGas/extracted-msg-fil_1_storageminer-WithdrawBalance-SysErrOutOfGas-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storagepower/CreateMiner/16/extracted-msg-fil_1_storagepower-CreateMiner-16-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storagepower/CreateMiner/SysErrOutOfGas/extracted-msg-fil_1_storagepower-CreateMiner-SysErrOutOfGas-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/DeclareFaults/Ok/ext-0001-fil_1_storageminer-DeclareFaults-Ok-").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/PreCommitSector/19/ext-0001-fil_1_storageminer-PreCommitSector-19-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_storageminer/PreCommitSector/Ok/ext-0001-fil_1_storageminer-PreCommitSector-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storagemarket/PublishStorageDeals/19/extracted-msg-fil_1_storagemarket-PublishStorageDeals-19-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/AddLockedFund/Ok/extracted-msg-fil_1_storageminer-AddLockedFund-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/DeclareFaultsRecovered/16/extracted-msg-fil_1_storageminer-DeclareFaultsRecovered-16-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storagepower/CreateMiner/Ok/extracted-msg-fil_1_storagepower-CreateMiner-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0001-initial-extraction/fil_1_account/Send/Ok/ext-0001-fil_1_account-Send-Ok-3").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/Send/SysErrOutOfGas/extracted-msg-fil_1_storageminer-Send-SysErrOutOfGas-*").unwrap(),
        Regex::new(r"test-vectors/corpus/extracted/0004-coverage-boost/fil_1_storageminer/DeclareFaults/Ok/extracted-msg-fil_1_storageminer-DeclareFaults-Ok-*").unwrap(),
        Regex::new(r"test-vectors/corpus/msg_application/gas_cost--msg-ok-secp-bls-gas-costs.json").unwrap(),
        Regex::new(r"test-vectors/corpus/msg_application/duplicates--messages-deduplicated.json").unwrap(),
        Regex::new(r"test-vectors/corpus/reward/penalties--not-penalized-insufficient-gas-for-return.json").unwrap(),
        Regex::new(r"test-vectors/corpus/reward/penalties--penalize-insufficient-balance-to-cover-gas.json").unwrap(),
        Regex::new(r"test-vectors/corpus/reward/penalties--not-penalized-insufficient-balance-to-cover-gas-and-transfer.json").unwrap(),
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

        let (ret, post_root) = execute_message(&bs, &to_chain_msg(msg), &root, epoch, &selector)?;
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

// This might be changed to be encoded into vector, matching go runner for now
fn to_chain_msg(msg: UnsignedMessage) -> ChainMessage {
    if msg.from().protocol() == Protocol::Secp256k1 {
        ChainMessage::Signed(SignedMessage {
            message: msg,
            signature: Signature::new_secp256k1(vec![0; 65]),
        })
    } else {
        ChainMessage::Unsigned(msg)
    }
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

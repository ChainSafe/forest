// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "submodule_tests")]
use specs_actors::*;

use walkdir::{WalkDir, DirEntry};
use cid::Cid;

use std::error::Error as StdError;
use std::fs::File;
use std::io::BufReader;

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

async fn execute_message_vector(
    car: &[u8],
    base_fee: Option<f64>,
    circ_supply: Option<f64>,
    apply_messages: &[ApplyMessage],
    postconditions: &PostConditions,
    variant: &Variant,
) -> Result<(), Box<dyn StdError>> {
    Ok(())
}

#[async_std::test]
async fn specs_actors_test_runner() {
    pretty_env_logger::init();

    let walker = WalkDir::new("specs-actors/test-vectors/determinism").into_iter();
    let mut failed = vec![];
    let mut succeeded = 0;
    for entry in walker.filter_map(|e| e.ok()).filter(is_valid_file) {
        let file = File::open(entry.path()).unwrap();
        let reader = BufReader::new(file);
        let test_name = entry.path().display();
        let vector: TestVector = serde_json::from_reader(reader).unwrap();

        for variant in vector.pre_conditions.variants {
            if let Err(e) = execute_message_vector(
                &vector.car,
                vector.pre_conditions.base_fee,
                vector.pre_conditions.circ_supply,
                &vector.apply_messages,
                &vector.post_conditions,
                &variant,
            ).await {
                failed.push((
                    format!("{} variant {}", test_name, vector.meta.id),
                    vector.meta.clone(),
                    e,
                ));
            } else {
                println!("{} succeeded", test_name);
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
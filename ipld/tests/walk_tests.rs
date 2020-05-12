// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "json")]

use async_trait::async_trait;
use cid::{json as cidjson, multihash::Blake2b256, Cid};
use db::MemoryDB;
use forest_ipld::json::{self, IpldJson};
use forest_ipld::selector::{LinkResolver, Selector, VisitReason};
use forest_ipld::Ipld;
use ipld_blockstore::BlockStore;
use serde::Deserialize;
use std::fs::File;
use std::io::BufReader;
use std::sync::{Arc, Mutex};

/// Type to ignore the specifics of a list or map for JSON tests
#[derive(Deserialize, Debug, Clone)]
enum IpldValue {
    #[serde(rename = "null")]
    Null,
    #[serde(rename = "bool")]
    Bool(bool),
    #[serde(rename = "integer")]
    Integer(i128),
    #[serde(rename = "float")]
    Float(f64),
    #[serde(rename = "string")]
    String(String),
    #[serde(rename = "bytes")]
    Bytes(Vec<u8>),
    #[serde(rename = "list")]
    List,
    #[serde(rename = "map")]
    Map,
    #[serde(rename = "link", with = "cidjson")]
    Link(Cid),
}

#[derive(Deserialize, Clone)]
struct ExpectVisit {
    path: String,
    node: IpldValue,
    matched: bool,
}

#[derive(Deserialize)]
struct TestVector {
    description: Option<String>,
    #[serde(with = "json")]
    ipld: Ipld,
    selector: Selector,
    expect_visit: Vec<ExpectVisit>,
    cbor_ipld_storage: Option<Vec<IpldJson>>,
}

fn check_ipld(ipld: &Ipld, value: &IpldValue) -> bool {
    use IpldValue::*;
    match (ipld, value) {
        (&Ipld::Map(_), &Map) => true,
        (&Ipld::List(_), &List) => true,
        (&Ipld::Null, &Null) => true,
        (&Ipld::Bool(ref a), &Bool(ref b)) => a == b,
        (&Ipld::Integer(ref a), &Integer(ref b)) => a == b,
        (&Ipld::Float(ref a), &Float(ref b)) => a == b,
        (&Ipld::String(ref a), &String(ref b)) => a == b,
        (&Ipld::Bytes(ref a), &Bytes(ref b)) => a == b,
        (&Ipld::Link(ref a), &Link(ref b)) => a == b,
        _ => false,
    }
}

fn check_matched(reason: VisitReason, matched: bool) -> bool {
    match (reason, matched) {
        (VisitReason::SelectionMatch, true) => true,
        (VisitReason::SelectionCandidate, false) => true,
        _ => false,
    }
}

#[derive(Clone)]
struct TestLinkResolver(MemoryDB);

#[async_trait]
impl LinkResolver for TestLinkResolver {
    async fn load_link(&self, link: &Cid) -> Result<Option<Ipld>, String> {
        self.0.get(link).map_err(|e| e.to_string())
    }
}

async fn process_vector(tv: TestVector) -> Result<(), String> {
    // Setup resolver with any ipld nodes to store
    let resolver = tv.cbor_ipld_storage.map(|ipld_storage| {
        let storage = MemoryDB::default();
        for IpldJson(i) in ipld_storage {
            storage.put(&i, Blake2b256).unwrap();
        }
        TestLinkResolver(storage)
    });

    // Index to ensure that the callback can check against the expectations
    let index = Arc::new(Mutex::new(0));
    let expect = tv.expect_visit.clone();
    let description = tv
        .description
        .clone()
        .unwrap_or("unnamed test case".to_owned());

    tv.selector
        .walk_all(
            &tv.ipld,
            resolver,
            |prog, ipld, reason| -> Result<(), String> {
                let mut idx = index.lock().unwrap();
                let exp = &expect[*idx];
                let current_path = prog.path().to_string();
                if current_path != exp.path {
                    return Err(format!(
                        "{:?} at (idx: {}) does not match {:?}",
                        current_path, *idx, exp.path
                    ));
                }
                if !check_ipld(ipld, &exp.node) {
                    return Err(format!(
                        "{:?} at (idx: {}) does not match {:?}",
                        ipld, *idx, exp.node
                    ));
                }
                if !check_matched(reason, exp.matched) {
                    return Err(format!(
                        "{:?} at (idx: {}) does not match {:?}",
                        reason, *idx, exp.matched
                    ));
                }
                *idx += 1;
                Ok(())
            },
        )
        .await
        .map_err(|e| format!("({}) failed, reason: {}", description, e.to_string()))?;

    // Ensure all expected traversals were checked
    let current_idx = *index.lock().unwrap();
    if expect.len() != current_idx {
        Err(format!(
            "{}: Did not traverse all expected nodes (expected: {}) (current: {})",
            description,
            expect.len(),
            current_idx
        ))
    } else {
        Ok(())
    }
}

async fn process_file(file: &str) -> Result<(), String> {
    let file = File::open(file).unwrap();
    let reader = BufReader::new(file);
    let vectors: Vec<TestVector> =
        serde_json::from_reader(reader).expect("Test vector deserialization failed");
    for tv in vectors.into_iter() {
        process_vector(tv).await?
    }

    Ok(())
}

#[async_std::test]
async fn selector_explore_tests() {
    process_file("./tests/selector_walk.json").await.unwrap();
}

#[async_std::test]
async fn selector_explore_links_tests() {
    process_file("./tests/selector_walk_links.json")
        .await
        .unwrap();
}

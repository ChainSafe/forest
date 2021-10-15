// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "submodule_tests")]

use async_trait::async_trait;
use cid::{Cid, Code::Blake2b256};
use db::MemoryDB;
use forest_ipld::json::{self, IpldJson};
use forest_ipld::selector::{LastBlockInfo, LinkResolver, Selector, VisitReason};
use forest_ipld::{Ipld, Path};
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
    #[serde(rename = "link", with = "cid::json")]
    Link(Cid),
}

#[derive(Deserialize, Clone)]
struct ExpectVisit {
    #[serde(with = "path_json")]
    path: Path,
    node: IpldValue,
    #[serde(with = "last_block_json", default)]
    last_block: Option<LastBlockInfo>,
    matched: bool,
}

mod path_json {
    use super::Path;
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Path, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: std::borrow::Cow<'de, str> = Deserialize::deserialize(deserializer)?;
        Ok(Path::from(s.as_ref()))
    }
}

mod last_block_json {
    use super::LastBlockInfo;
    use super::Path;
    use cid::Cid;
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<LastBlockInfo>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct LastBlockDe {
            #[serde(with = "super::path_json")]
            path: Path,
            #[serde(with = "cid::json")]
            link: Cid,
        }
        match Deserialize::deserialize(deserializer)? {
            Some(LastBlockDe { path, link }) => Ok(Some(LastBlockInfo { path, link })),
            None => Ok(None),
        }
    }
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
    async fn load_link(&mut self, link: &Cid) -> Result<Option<Ipld>, String> {
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
                // Current path
                if prog.path() != &exp.path {
                    return Err(format!(
                        "{:?} at (idx: {}) does not match {:?}",
                        prog.path(),
                        *idx,
                        exp.path
                    ));
                }
                // Current Ipld node
                if !check_ipld(ipld, &exp.node) {
                    return Err(format!(
                        "{:?} at (idx: {}) does not match {:?}",
                        ipld, *idx, exp.node
                    ));
                }
                // Match boolean against visit reason
                if !check_matched(reason, exp.matched) {
                    return Err(format!(
                        "{:?} at (idx: {}) does not match {:?}",
                        reason, *idx, exp.matched
                    ));
                }
                // Check last block information
                if prog.last_block() != exp.last_block.as_ref() {
                    return Err(format!(
                        "{:?} at (idx: {}) does not match {:?}",
                        prog.last_block(),
                        *idx,
                        exp.last_block
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
    process_file("./tests/ipld-traversal-vectors/selector_walk.json")
        .await
        .unwrap();
}

#[async_std::test]
async fn selector_explore_links_tests() {
    process_file("./tests/ipld-traversal-vectors/selector_walk_links.json")
        .await
        .unwrap();
}

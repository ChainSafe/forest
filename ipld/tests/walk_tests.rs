// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    fs::File,
    io::BufReader,
    sync::atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use cid::{multihash::Code::Blake2b256, Cid};
use forest_db::MemoryDB;
use forest_ipld::{
    json::{self, IpldJson},
    selector::{LastBlockInfo, LinkResolver, Selector, VisitReason},
};
use fvm_ipld_encoding::CborStore;
use libipld::Path;
use libipld_core::ipld::Ipld;
use serde::Deserialize;

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
    #[serde(rename = "link", with = "forest_json::cid")]
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
    use serde::{Deserialize, Deserializer};

    use super::Path;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Path, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: std::borrow::Cow<'de, str> = Deserialize::deserialize(deserializer)?;
        Ok(Path::from(s.as_ref()))
    }
}

mod last_block_json {
    use cid::Cid;
    use serde::{Deserialize, Deserializer};

    use super::{LastBlockInfo, Path};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<LastBlockInfo>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct LastBlockDe {
            #[serde(with = "super::path_json")]
            path: Path,
            #[serde(with = "forest_json::cid")]
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
        (Ipld::Bool(a), Bool(b)) => a == b,
        (Ipld::Integer(a), Integer(b)) => a == b,
        (Ipld::Float(a), Float(b)) => a == b,
        (Ipld::String(a), String(b)) => a == b,
        (Ipld::Bytes(a), Bytes(b)) => a == b,
        (Ipld::Link(a), Link(b)) => a == b,
        _ => false,
    }
}

fn check_matched(reason: VisitReason, matched: bool) -> bool {
    matches!(
        (reason, matched),
        (VisitReason::SelectionMatch, true) | (VisitReason::SelectionCandidate, false)
    )
}

#[derive(Clone)]
struct TestLinkResolver(MemoryDB);

#[async_trait]
impl LinkResolver for TestLinkResolver {
    async fn load_link(&mut self, link: &Cid) -> Result<Option<Ipld>, String> {
        self.0.get_cbor(link).map_err(|e| e.to_string())
    }
}

async fn process_vector(tv: TestVector) -> Result<(), String> {
    // Setup resolver with any ipld nodes to store
    let resolver = tv.cbor_ipld_storage.map(|ipld_storage| {
        let storage = MemoryDB::default();
        for IpldJson(i) in ipld_storage {
            storage.put_cbor(&i, Blake2b256).unwrap();
        }
        TestLinkResolver(storage)
    });

    // Index to ensure that the callback can check against the expectations
    let index = AtomicUsize::new(0);
    let expect = tv.expect_visit.clone();
    let description = tv
        .description
        .clone()
        .unwrap_or_else(|| "unnamed test case".to_owned());

    tv.selector
        .walk_all(
            &tv.ipld,
            resolver,
            |prog, ipld, reason| -> Result<(), String> {
                let idx = index.load(Ordering::Relaxed);
                let exp = &expect[idx];
                // Current path
                if prog.path() != &exp.path {
                    return Err(format!(
                        "{:?} at (idx: {}) does not match {:?}",
                        prog.path(),
                        idx,
                        exp.path
                    ));
                }
                // Current Ipld node
                if !check_ipld(ipld, &exp.node) {
                    return Err(format!(
                        "{:?} at (idx: {}) does not match {:?}",
                        ipld, idx, exp.node
                    ));
                }
                // Match boolean against visit reason
                if !check_matched(reason, exp.matched) {
                    return Err(format!(
                        "{:?} at (idx: {}) does not match {:?}",
                        reason, idx, exp.matched
                    ));
                }
                // Check last block information
                if prog.last_block() != exp.last_block.as_ref() {
                    return Err(format!(
                        "{:?} at (idx: {}) does not match {:?}",
                        prog.last_block(),
                        idx,
                        exp.last_block
                    ));
                }
                index.fetch_add(1, Ordering::Relaxed);
                Ok(())
            },
        )
        .await
        .map_err(|e| format!("({description}) failed, reason: {e}"))?;

    // Ensure all expected traversals were checked
    let current_idx = index.into_inner();
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

#[tokio::test]
async fn selector_explore_tests() {
    process_file("./tests/ipld-traversal-vectors/selector_walk.json")
        .await
        .unwrap();
}

#[tokio::test]
async fn selector_explore_links_tests() {
    process_file("./tests/ipld-traversal-vectors/selector_walk_links.json")
        .await
        .unwrap();
}

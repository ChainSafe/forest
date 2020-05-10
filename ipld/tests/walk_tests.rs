// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use forest_ipld::selector::{Progress, Selector, VisitReason};
use forest_ipld::Ipld;
use serde::Deserialize;
use std::fs::File;
use std::io::BufReader;
use std::sync::{Arc, Mutex};

#[derive(Deserialize, Debug, Clone)]
pub enum IpldValue {
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
    #[serde(rename = "link")]
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
    ipld: Ipld,
    selector: Selector,
    expect_visit: Vec<ExpectVisit>,
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

async fn process_vector(tv: TestVector) -> Result<(), String> {
    let index = Arc::new(Mutex::new(0));
    let expect = tv.expect_visit.clone();
    let description = tv.description.clone();
    tv.selector
        .walk_all(
            &tv.ipld,
            None,
            |prog: &Progress<()>, ipld, reason| -> Result<(), String> {
                let mut idx = index.lock().unwrap();
                let exp = &expect[*idx];
                if !check_ipld(ipld, &exp.node) {
                    return Err(format!("{:?} does not match {:?}", ipld, exp.node));
                }
                if !check_matched(reason, exp.matched) {
                    return Err(format!("{:?} does not match {:?}", reason, exp.matched));
                }
                let current_path = prog.path().to_string();
                if current_path != exp.path {
                    return Err(format!("{:?} does not match {:?}", current_path, exp.path));
                }
                *idx += 1;
                Ok(())
            },
        )
        .await
        .map_err(|e| {
            format!(
                "({}) failed, reason: {}",
                description.unwrap_or("unnamed test case".to_owned()),
                e.to_string()
            )
        })
}

#[async_std::test]
async fn selector_explore_tests() {
    let file = File::open("./tests/selector_walk.json").unwrap();
    let reader = BufReader::new(file);
    let vectors: Vec<TestVector> =
        serde_json::from_reader(reader).expect("Test vector deserialization failed");
    for tv in vectors.into_iter() {
        process_vector(tv).await.unwrap()
    }
}

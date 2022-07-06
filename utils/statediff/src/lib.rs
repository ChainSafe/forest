// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// use actor::account;
// use actor::market;
// use actor::miner;
// use actor::power;
use address::Address;
use blockstore::resolve::resolve_cids_recursive;
use blockstore::BlockStore;
use cid::{json::CidJson, Cid};
use colored::*;
use difference::{Changeset, Difference};
use ipld::json::{IpldJson, IpldJsonRef};
use ipld::Ipld;
use serde::{Deserialize, Serialize};
use state_tree::StateTree;
use vm::ActorState;

use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::io::stdout;
use std::io::Write;

#[derive(Serialize, Deserialize)]
struct ActorStateResolved {
    code: CidJson,
    sequence: u64,
    balance: String,
    state: IpldJson,
}

fn actor_to_resolved(
    bs: &impl BlockStore,
    actor: &ActorState,
    depth: Option<u64>,
) -> ActorStateResolved {
    let resolved =
        resolve_cids_recursive(bs, &actor.state, depth).unwrap_or(Ipld::Link(actor.state));
    ActorStateResolved {
        state: IpldJson(resolved),
        code: CidJson(actor.code),
        balance: actor.balance.to_string(),
        sequence: actor.sequence,
    }
}

fn root_to_state_map<BS: BlockStore>(
    bs: &BS,
    root: &Cid,
) -> Result<HashMap<Address, ActorState>, anyhow::Error> {
    let mut actors = HashMap::default();
    let state_tree = StateTree::new_from_root(bs, root)?;
    state_tree.for_each(|addr: Address, actor: &ActorState| {
        actors.insert(addr, actor.clone());
        Ok(())
    })?;

    Ok(actors)
}

/// Tries to resolve state tree actors, if all data exists in store.
/// The actors hamt is hard to parse in a diff, so this attempts to remedy this.
/// This function will only print the actors that are added, removed, or changed so it
/// can be used on large state trees.
fn try_print_actor_states<BS: BlockStore>(
    bs: &BS,
    root: &Cid,
    expected_root: &Cid,
    depth: Option<u64>,
) -> Result<(), anyhow::Error> {
    // For now, resolving to a map, because we need to use go implementation's inefficient caching
    // this would probably be faster in most cases.
    let mut e_state = root_to_state_map(bs, expected_root)?;

    // Compare state with expected
    let state_tree = StateTree::new_from_root(bs, root)?;

    state_tree.for_each(|addr: Address, actor: &ActorState| {
        let calc_pp = pp_actor_state(bs, actor, depth)?;

        if let Some(other) = e_state.remove(&addr) {
            if &other != actor {
                let expected_pp = pp_actor_state(bs, &other, depth)?;
                let Changeset { diffs, .. } = Changeset::new(&expected_pp, &calc_pp, ",");
                let stdout = stdout();
                let mut handle = stdout.lock();
                writeln!(handle, "Address {} changed: ", addr)?;
                print_diffs(&mut handle, &diffs)?;
            }
        } else {
            // Added actor, print out the json format actor state.
            println!("{}", format!("+ Address {}:\n{}", addr, calc_pp).green());
        }

        Ok(())
    })?;

    // Print all addresses that no longer have actor state
    for (addr, state) in e_state.into_iter() {
        let expected_json = serde_json::to_string_pretty(&actor_to_resolved(bs, &state, depth))?;
        println!(
            "{}",
            format!("- Address {}:\n{}", addr, expected_json).red()
        )
    }

    Ok(())
}

fn pp_actor_state(
    bs: &impl BlockStore,
    state: &ActorState,
    depth: Option<u64>,
) -> Result<String, anyhow::Error> {
    let resolved = actor_to_resolved(bs, state, depth);
    // let ipld = &resolved.state.0;
    let mut buffer = String::new();

    writeln!(&mut buffer, "{:?}", state)?;

    // FIXME: Use the actor interface to load and pretty print the actor states.
    //        Tracker: https://github.com/ChainSafe/forest/issues/1561
    // if let Ok(miner_state) = ipld::from_ipld::<miner::State>(ipld.clone()) {
    //     write!(&mut buffer, "{:?}", miner_state)?;
    // } else if let Ok(account_state) = ipld::from_ipld::<account::State>(ipld.clone()) {
    //     write!(&mut buffer, "{:?}", account_state)?;
    // } else if let Ok(state) = ipld::from_ipld::<power::State>(ipld.clone()) {
    //     write!(&mut buffer, "{:?}", state)?;
    // } else if let Ok(state) = ipld::from_ipld::<market::State>(ipld.clone()) {
    //     write!(&mut buffer, "{:?}", state)?;
    // } else {
    buffer += &serde_json::to_string_pretty(&resolved)?;
    // }
    Ok(buffer)
}

fn print_diffs(handle: &mut impl Write, diffs: &[Difference]) -> std::io::Result<()> {
    for diff in diffs.iter() {
        match diff {
            Difference::Same(x) => writeln!(handle, " {}", x)?,
            Difference::Add(x) => writeln!(handle, "{}", format!("+{}", x).green())?,
            Difference::Rem(x) => writeln!(handle, "{}", format!("-{}", x).red())?,
        }
    }
    Ok(())
}

pub fn print_actor_diff<BS: BlockStore>(
    bs: &BS,
    expected: &ActorState,
    actual: &ActorState,
    depth: Option<u64>,
) -> Result<(), anyhow::Error> {
    let expected_pp = pp_actor_state(bs, expected, depth)?;
    let actual_pp = pp_actor_state(bs, actual, depth)?;

    let Changeset { diffs, .. } = Changeset::new(&expected_pp, &actual_pp, "\n");
    let stdout = stdout();
    let mut handle = stdout.lock();
    print_diffs(&mut handle, &diffs)?;
    Ok(())
}

/// Prints a diff of the resolved state tree.
/// If the actor's Hamt cannot be loaded, base ipld resolution is given.
pub fn print_state_diff<BS>(
    bs: &BS,
    root: &Cid,
    expected_root: &Cid,
    depth: Option<u64>,
) -> Result<(), anyhow::Error>
where
    BS: BlockStore,
{
    eprintln!(
        "StateDiff:\n  Expected: {}\n  Root: {}",
        expected_root, root
    );
    if let Err(e) = try_print_actor_states(bs, root, expected_root, depth) {
        println!(
            "Could not resolve actor states: {}\nUsing default resolution:",
            e
        );
        let expected = resolve_cids_recursive(bs, expected_root, depth)?;
        let actual = resolve_cids_recursive(bs, root, depth)?;

        let expected_json = serde_json::to_string_pretty(&IpldJsonRef(&expected))?;
        let actual_json = serde_json::to_string_pretty(&IpldJsonRef(&actual))?;

        let Changeset { diffs, .. } = Changeset::new(&expected_json, &actual_json, "\n");
        let stdout = stdout();
        let mut handle = stdout.lock();
        print_diffs(&mut handle, &diffs)?
    }

    Ok(())
}

fn is_match(a: Ipld, b: Ipld) -> bool {
    match a {
        Ipld::Null => if let Ipld::Null = b {
            return true;
        },
        Ipld::Bool(value) => if let Ipld::Bool(other_value) = b {
            return value == other_value;
        }
        Ipld::Integer(value) => if let Ipld::Integer(other_value) = b {
            return value == other_value;
        }
        Ipld::Float(value) => if let Ipld::Float(other_value) = b {
            return value == other_value;
        }
        Ipld::String(value) => if let Ipld::String(other_value) = b {
            return value == other_value;
        }
        Ipld::Bytes(value) => if let Ipld::Bytes(other_value) = b {
            return value == other_value;
        }
        Ipld::List(value) => if let Ipld::List(other_value) = b {
            return value == other_value;
        }
        Ipld::Map(value) => if let Ipld::Map(other_value) = b {
            return value == other_value;
        }
        Ipld::Link(value) => if let Ipld::Link(other_value) = b {
            return value == other_value;
        }
    }
    false
}

pub fn print_ipld_diff<BS>(
    bs: &BS,
    pre: &Cid,
    post: &Option<Cid>,
    depth: Option<u64>,
) -> Result<(), anyhow::Error>
where
    BS: BlockStore,
{
    use blocks::BlockHeader;

    let left = bs.get_anyhow::<BlockHeader>(&pre)?;
    println!("{:?}", left.unwrap());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use db::MemoryDB;
    use blockstore::BlockStore;
    use std::path::{Path, PathBuf};
    use db::rocks::RocksDb;
    use db::rocks_config::RocksDbConfig;

    fn get_db() -> RocksDb {
        let db_path = PathBuf::from("/Users/guillaume/Library/Application Support/com.ChainSafe.Forest/mainnet/db");
        let db = db::rocks::RocksDb::open(db_path, &RocksDbConfig::default())
            .expect("Opening RocksDB must succeed");

        db
    }

    #[async_std::test]
    async fn basic_diff_test() {
        let a = ipld::ipld!({
            "code": 200,
            "success": true,
            "link": Link("QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n".parse().unwrap()),
            "bytes": Bytes(vec![0x1, 0xfa, 0x8b]),
            "payload": {
                "features": [
                    "serde",
                    "ipld"
                ]
                }
            });

        let b = ipld::ipld!({
            "code": 200,
            "success": true,
            "link": Link("QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n".parse().unwrap()),
            "bytes": Bytes(vec![0x1, 0xff, 0x8b]),
            "payload": {
                "features": [
                    "serde",
                    "ipld"
                ]
                }
            });

        // look for local rocksdb
        let db = get_db();
        let cid = Cid::try_from("bafy2bzaceb5vqcmq4ejdo473mc3dgre6h4wur5nulo6c2ig7f4jyj4ukq2mqe").unwrap();
        print_ipld_diff(&db, &cid, &None, None);
    }
}

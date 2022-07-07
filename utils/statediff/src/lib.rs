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
use std::collections::BTreeMap;
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

fn same_variant(a: &Ipld, b: &Ipld) -> bool {
    match a {
        Ipld::Null => if let Ipld::Null = b {
            return true;
        },
        Ipld::Bool(_) => if let Ipld::Bool(_) = b {
            return true;
        }
        Ipld::Integer(_) => if let Ipld::Integer(_) = b {
            return true;
        }
        Ipld::Float(_) => if let Ipld::Float(_) = b {
            return true;
        }
        Ipld::String(_) => if let Ipld::String(_) = b {
            return true;
        }
        Ipld::Bytes(_) => if let Ipld::Bytes(_) = b {
            return true;
        }
        Ipld::List(_) => if let Ipld::List(_) = b {
            return true;
        }
        Ipld::Map(_) => if let Ipld::Map(_) = b {
            return true;
        }
        Ipld::Link(_) => if let Ipld::Link(_) = b {
            return true;
        }
    }
    false
}

fn is_plain_data(node: &Ipld) -> bool {
    match node {
        Ipld::List(_) | Ipld::Map(_) | Ipld::Link(_)  => false,
        _ => true,
    }
}

fn get_links(a: &Ipld, b: &Ipld) -> Option<(Cid, Cid)> {
    match a {
        Ipld::Link(cid_a) => if let Ipld::Link(cid_b) = b {
            return Some((*cid_a, *cid_b));
        }
        _ => (),
    }
    return None;
}

fn get_lists(a: &Ipld, b: &Ipld) -> Option<(Vec<Ipld>, Vec<Ipld>)> {
    match a {
        Ipld::List(vec_a) => if let Ipld::List(vec_b) = b {
            return Some((vec_a.clone(), vec_b.clone()));
        }
        _ => (),
    }
    return None;
}

fn get_maps(a: &Ipld, b: &Ipld) -> Option<(BTreeMap<String, Ipld>, BTreeMap<String, Ipld>)> {
    match a {
        Ipld::Map(map_a) => if let Ipld::Map(map_b) = b {
            return Some((map_a.clone(), map_b.clone()));
        }
        _ => (),
    }
    return None;
}

#[derive(Debug)]
enum ListDiff {
    Deletion(usize),
    Insertion(usize),
    Updates(Vec<usize>),
}

#[derive(Debug)]
enum MapDiff {
    Deletion(usize),
    Insertion(usize),
    Updates(Vec<String>),
    Other,
}

#[derive(Debug)]
enum Diff {
    KeyNotFound,
    Variant,
    PlainData,
    List(ListDiff),
    Map(MapDiff),
}

struct DiffMap {
    entries: BTreeMap<(Cid, Cid), Vec<Diff>>
}

impl DiffMap {
    fn new() -> Self {
        DiffMap {
            entries: BTreeMap::new()
        }
    }

    fn insert(&mut self, key: (Cid, Cid), value: Diff) {
        if let Some(v) = self.entries.get_mut(&key) {
            v.push(value);
        } else {
            self.entries.insert(key, vec!(value));
        }
    }
}

fn find_vec_diffs<BS>(bs: &BS, cid_a: &Cid, cid_b: &Cid, a: &Vec<Ipld>, b: &Vec<Ipld>, level: usize, diffs: &mut DiffMap) -> Result<(), anyhow::Error>
where
    BS: BlockStore
{
    use std::cmp::Ordering;

    match b.len().cmp(&a.len()) {
        Ordering::Equal => {
            let mut indices: Vec<usize> = vec![];
            for (i, (a, b)) in a.iter().zip(b.iter()).enumerate() {
                if find_ipld_diffs(bs, cid_a, cid_b, a, b, level, diffs)? {
                    indices.push(i);
                }
            }
            diffs.insert((cid_a.clone(), cid_b.clone()), Diff::List(ListDiff::Updates(indices)));
        },
        Ordering::Less => {
            let n = a.len() - b.len();
            diffs.insert((cid_a.clone(), cid_b.clone()), Diff::List(ListDiff::Deletion(n)));
        },
        Ordering::Greater => {
            let n = b.len() - a.len();
            diffs.insert((cid_a.clone(), cid_b.clone()), Diff::List(ListDiff::Insertion(n)));
        },
    }

    Ok(())
}

fn find_map_diffs<BS>(bs: &BS, cid_a: &Cid, cid_b: &Cid, a: &BTreeMap<String, Ipld>, b: &BTreeMap<String, Ipld>, level: usize, diffs: &mut DiffMap) -> Result<(), anyhow::Error>
where
    BS: BlockStore
{
    use std::cmp::Ordering;

    match b.len().cmp(&a.len()) {
        Ordering::Equal => {
            let mut keys: Vec<String> = vec![];
            for (k, node_a) in a.iter() {
                if let Some(node_b) = b.get(k) {
                    if find_ipld_diffs(bs, cid_a, cid_b, node_a, node_b, level, diffs)? {
                        keys.push(k.clone());
                    }
                }
            }
            if !keys.is_empty() {
                diffs.insert((cid_a.clone(), cid_b.clone()), Diff::Map(MapDiff::Updates(keys)));
            } else {
                // result in a mix of new insertions, deletions
                diffs.insert((cid_a.clone(), cid_b.clone()), Diff::Map(MapDiff::Other));
            }
        },
        Ordering::Less => {
            let n = a.len() - b.len();
            diffs.insert((cid_a.clone(), cid_b.clone()), Diff::Map(MapDiff::Deletion(n)));
        },
        Ordering::Greater => {
            let n = b.len() - a.len();
            diffs.insert((cid_a.clone(), cid_b.clone()), Diff::Map(MapDiff::Insertion(n)));
        },
    }

    Ok(())
}

fn find_ipld_diffs<BS>(bs: &BS, cid_a: &Cid, cid_b: &Cid, a: &Ipld, b: &Ipld, level: usize, diffs: &mut DiffMap) -> Result<bool, anyhow::Error>
where
    BS: BlockStore,
{
    if same_variant(a, b) {
        if a != b {
            if is_plain_data(a) {
                diffs.insert((cid_a.clone(), cid_b.clone()), Diff::PlainData);
            }
            else {
                if let Some((new_cid_a, new_cid_b)) = get_links(a, b) {
                    find_cid_diffs(bs, &new_cid_a, &new_cid_b, level+1, diffs)?;
                } else if let Some((vec_a, vec_b)) = get_lists(a, b) {
                    find_vec_diffs(bs, &cid_a, &cid_b, &vec_a, &vec_b, level, diffs)?;
                } else if let Some((map_a, map_b)) = get_maps(a, b) {
                    find_map_diffs(bs, &cid_a, &cid_b, &map_a, &map_b, level, diffs)?;
                } else {
                    unreachable!();
                }
            }
            return Ok(true);
        } else {
            // do nothing
        }
    } else {
        diffs.insert((cid_a.clone(), cid_b.clone()), Diff::Variant);
        return Ok(true);
    }
    Ok(false)
}

fn find_cid_diffs<BS>(bs: &BS, cid_a: &Cid, cid_b: &Cid, level: usize, diffs: &mut DiffMap) -> Result<(), anyhow::Error>
where
    BS: BlockStore,
{
    let node_a = bs.get_anyhow::<Ipld>(&cid_a)?;
    let node_b = bs.get_anyhow::<Ipld>(&cid_b)?;
    if let Some(a) = node_a {
        if let Some(b) = node_b {
            find_ipld_diffs(bs, &cid_a, &cid_b, &a, &b, level, diffs)?;
        } else {
            diffs.insert((cid_a.clone(), cid_b.clone()), Diff::KeyNotFound);
        }
    } else {
        diffs.insert((cid_a.clone(), cid_b.clone()), Diff::KeyNotFound);
    }
    Ok(())
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

    if let Some(post) = post {
        // first check if state root
        StateTree::new_from_root(bs, &pre)
            .expect(&format!("expecting {} to be a state root", pre));

        StateTree::new_from_root(bs, &post)
            .expect(&format!("expecting {} to be a state root", post));

        let mut diffs = DiffMap::new();
        find_cid_diffs(bs, &pre, &post, 0, &mut diffs).unwrap();
        println!("found {} differences:", diffs.entries.len());
        for diff in diffs.entries.iter() {
            println!("{:?}", diff);
            let ((a, b), kind) = diff;
            // if let PlainData = kind {
            //     try_print_actor_states(bs, &a, &b, None);
            // }
        }
    } else {
        // just print state root
        let bh = bs.get_anyhow::<BlockHeader>(&pre)?;
        println!("state_root({pre}): {}", bh.unwrap().state_root());
    }

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
        // look for a local rocksdb
        let db = get_db();
        //let cid = Cid::try_from("bafy2bzaceb5vqcmq4ejdo473mc3dgre6h4wur5nulo6c2ig7f4jyj4ukq2mqe").unwrap();

        // Height #1927022
        // state_root: bafy2bzacechcvphowekjggyz7aayyprwsipgkcedobbuw7k77aigcx7sw67hc
        let cid = Cid::try_from("bafy2bzaceadvwb7wfkvs25ih7eh6znn2xm5qzeldib7d5se5kkny2h4wk4yzo").unwrap();
        print_ipld_diff(&db, &cid, &None, None);

        // Height #1927023
        // state_root: bafy2bzacebbb7orn4xpqcehtlmlqunt7ukaunr2xgusjmw4q5efrph2jo3cjc
        let cid = Cid::try_from("bafy2bzaceconxxu7hinpuacichclo5rhudezxcp54ftzbzfkywkxzfjlqxsse").unwrap();
        print_ipld_diff(&db, &cid, &None, None);

        let pre = Cid::try_from("bafy2bzacechcvphowekjggyz7aayyprwsipgkcedobbuw7k77aigcx7sw67hc").unwrap();
        let post = Cid::try_from("bafy2bzacebbb7orn4xpqcehtlmlqunt7ukaunr2xgusjmw4q5efrph2jo3cjc").unwrap();
        print_ipld_diff(&db, &pre, &Some(post), None);

        //print_state_diff(&db, &pre, &post, None).unwrap();
    }
}

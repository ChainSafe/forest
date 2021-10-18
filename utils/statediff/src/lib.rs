// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

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
use std::collections::HashMap;
use std::error::Error as StdError;
use vm::ActorState;

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
) -> Result<HashMap<Address, ActorState>, Box<dyn StdError>> {
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
) -> Result<(), Box<dyn StdError>> {
    // For now, resolving to a map, because we need to use go implementation's inefficient caching
    // this would probably be faster in most cases.
    let mut e_state = root_to_state_map(bs, expected_root)?;

    // Compare state with expected
    let state_tree = StateTree::new_from_root(bs, root)?;
    state_tree.for_each(|addr: Address, actor: &ActorState| {
        let calc_json = serde_json::to_string_pretty(&actor_to_resolved(bs, actor, depth))?;

        if let Some(other) = e_state.remove(&addr) {
            if &other != actor {
                let expected_json =
                    serde_json::to_string_pretty(&actor_to_resolved(bs, &other, depth))?;
                let Changeset { diffs, .. } = Changeset::new(&expected_json, &calc_json, "\n");
                println!("Address {} changed: ", addr);
                print_diffs(&diffs);
            }
        } else {
            // Added actor, print out the json format actor state.
            println!("{}", format!("+ Address {}:\n{}", addr, calc_json).green())
        }

        Ok(())
    })?;

    // Print all addresses that no longer have actor state
    for (addr, state) in e_state.into_iter() {
        let expected_json = serde_json::to_string_pretty(&actor_to_resolved(bs, &state, depth))?;
        println!(
            "{}",
            format!("- Address {}:\n{}", addr, expected_json).red()
        );
    }

    Ok(())
}

fn print_diffs(diffs: &[Difference]) {
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

/// Prints a diff of the resolved state tree.
/// If the actor's Hamt cannot be loaded, base ipld resolution is given.
pub fn print_state_diff<BS>(
    bs: &BS,
    root: &Cid,
    expected_root: &Cid,
    depth: Option<u64>,
) -> Result<(), Box<dyn StdError>>
where
    BS: BlockStore,
{
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
        print_diffs(&diffs);
    }

    Ok(())
}

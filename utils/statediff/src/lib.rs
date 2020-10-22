// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use blockstore::resolve::resolve_cids_recursive;
use blockstore::BlockStore;
use cid::{json::CidJson, Cid};
use colored::*;
use difference::{Changeset, Difference};
use fil_types::HAMT_BIT_WIDTH;
use ipld::json::{IpldJson, IpldJsonRef};
use ipld::Ipld;
use ipld_hamt::{BytesKey, Hamt};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::error::Error as StdError;
use vm::ActorState;

#[derive(Serialize, Deserialize)]
struct ActorStateResolved {
    code: CidJson,
    sequence: u64,
    balance: String,
    state: IpldJson,
}

fn root_to_state_map<BS: BlockStore>(
    bs: &BS,
    root: &Cid,
) -> Result<BTreeMap<String, ActorStateResolved>, Box<dyn StdError>> {
    let mut actors = BTreeMap::new();
    let hamt: Hamt<_, _> = Hamt::load_with_bit_width(root, bs, HAMT_BIT_WIDTH)?;
    hamt.for_each(|k: &BytesKey, actor: &ActorState| {
        let addr = Address::from_bytes(&k.0)?;

        let resolved = resolve_cids_recursive(bs, &actor.state)
            .unwrap_or_else(|_| Ipld::Link(actor.state.clone()));
        let resolved_state = ActorStateResolved {
            state: IpldJson(resolved),
            code: CidJson(actor.code.clone()),
            balance: actor.balance.to_string(),
            sequence: actor.sequence,
        };

        actors.insert(addr.to_string(), resolved_state);
        Ok(())
    })
    .unwrap();

    Ok(actors)
}

/// Tries to resolve state tree actors, if all data exists in store.
/// The actors hamt is hard to parse in a diff, so this attempts to remedy this.
fn try_resolve_actor_states<BS: BlockStore>(
    bs: &BS,
    root: &Cid,
    expected_root: &Cid,
) -> Result<Changeset, Box<dyn StdError>> {
    let e_state = root_to_state_map(bs, expected_root)?;
    let c_state = root_to_state_map(bs, root)?;

    let expected_json = serde_json::to_string_pretty(&e_state)?;
    let actual_json = serde_json::to_string_pretty(&c_state)?;

    Ok(Changeset::new(&expected_json, &actual_json, "\n"))
}

/// Prints a diff of the resolved state tree.
/// If the actor's Hamt cannot be loaded, base ipld resolution is given.
pub fn print_state_diff<BS>(
    bs: &BS,
    root: &Cid,
    expected_root: &Cid,
) -> Result<(), Box<dyn StdError>>
where
    BS: BlockStore,
{
    let Changeset { diffs, .. } = match try_resolve_actor_states(bs, root, expected_root) {
        Ok(cs) => cs,
        Err(e) => {
            println!(
                "Could not resolve actor states: {}\nUsing default resolution:",
                e
            );
            let expected = resolve_cids_recursive(bs, &expected_root)?;
            let actual = resolve_cids_recursive(bs, &root)?;

            let expected_json = serde_json::to_string_pretty(&IpldJsonRef(&expected))?;
            let actual_json = serde_json::to_string_pretty(&IpldJsonRef(&actual))?;

            Changeset::new(&expected_json, &actual_json, "\n")
        }
    };

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

    Ok(())
}

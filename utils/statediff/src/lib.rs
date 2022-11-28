// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod resolve;

use cid::Cid;
use colored::*;
use difference::{Changeset, Difference};
use forest_ipld::json::{IpldJson, IpldJsonRef};
use forest_json::cid::CidJson;
use fvm::state_tree::{ActorState, StateTree};
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::address::Address;
use libipld_core::ipld::Ipld;
use resolve::resolve_cids_recursive;
use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::io::stdout;
use std::io::Write;

use fil_actor_account_v9::State as AccountState;
use fil_actor_cron_v9::State as CronState;
use fil_actor_init_v9::State as InitState;
use fil_actor_market_v9::State as MarketState;
use fil_actor_miner_v9::State as MinerState;
use fil_actor_multisig_v9::State as MultiSigState;
use fil_actor_power_v9::State as PowerState;
use fil_actor_reward_v9::State as RewardState;
use fil_actor_system_v9::State as SystemState;

#[derive(Serialize, Deserialize)]
struct ActorStateResolved {
    code: CidJson,
    sequence: u64,
    balance: String,
    state: IpldJson,
}

fn actor_to_resolved(
    bs: &impl Blockstore,
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

fn root_to_state_map<BS: Blockstore>(
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
/// The actors HAMT is hard to parse in a diff, so this attempts to remedy this.
/// This function will only print the actors that are added, removed, or changed so it
/// can be used on large state trees.
fn try_print_actor_states<BS: Blockstore>(
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
    bs: &impl Blockstore,
    state: &ActorState,
    depth: Option<u64>,
) -> Result<String, anyhow::Error> {
    let resolved = actor_to_resolved(bs, state, depth);
    let ipld = &resolved.state.0;
    let mut buffer = String::new();

    writeln!(&mut buffer, "{:?}", state)?;

    // FIXME: Use the actor interface to load and pretty print the actor states.
    //        Tracker: https://github.com/ChainSafe/forest/issues/1561
    if let Ok(miner_state) = forest_ipld::from_ipld::<MinerState>(ipld.clone()) {
        write!(&mut buffer, "{:?}", miner_state)?;
        return Ok(buffer);
    }
    if let Ok(cron_state) = forest_ipld::from_ipld::<CronState>(ipld.clone()) {
        write!(&mut buffer, "{:?}", cron_state)?;
        return Ok(buffer);
    }
    if let Ok(account_state) = forest_ipld::from_ipld::<AccountState>(ipld.clone()) {
        write!(&mut buffer, "{:?}", account_state)?;
        return Ok(buffer);
    }
    if let Ok(power_state) = forest_ipld::from_ipld::<PowerState>(ipld.clone()) {
        write!(&mut buffer, "{:?}", power_state)?;
        return Ok(buffer);
    }
    if let Ok(init_state) = forest_ipld::from_ipld::<InitState>(ipld.clone()) {
        write!(&mut buffer, "{:?}", init_state)?;
        return Ok(buffer);
    }
    if let Ok(reward_state) = forest_ipld::from_ipld::<RewardState>(ipld.clone()) {
        write!(&mut buffer, "{:?}", reward_state)?;
        return Ok(buffer);
    }
    if let Ok(system_state) = forest_ipld::from_ipld::<SystemState>(ipld.clone()) {
        write!(&mut buffer, "{:?}", system_state)?;
        return Ok(buffer);
    }
    if let Ok(multi_sig_state) = forest_ipld::from_ipld::<MultiSigState>(ipld.clone()) {
        write!(&mut buffer, "{:?}", multi_sig_state)?;
        return Ok(buffer);
    }
    if let Ok(market_state) = forest_ipld::from_ipld::<MarketState>(ipld.clone()) {
        write!(&mut buffer, "{:?}", market_state)?;
        return Ok(buffer);
    }
    buffer += &serde_json::to_string_pretty(&resolved)?;
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

pub fn print_actor_diff<BS: Blockstore>(
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
/// If the actor's HAMT cannot be loaded, base IPLD resolution is given.
pub fn print_state_diff<BS>(
    bs: &BS,
    root: &Cid,
    expected_root: &Cid,
    depth: Option<u64>,
) -> Result<(), anyhow::Error>
where
    BS: Blockstore,
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

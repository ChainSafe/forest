// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod resolve;

use std::{
    fmt::Write as FmtWrite,
    io::{Write, stdout},
    sync::Arc,
};

use crate::shim::actors::{
    account::State as AccountState, cron::State as CronState, datacap::State as DatacapState,
    evm::State as EvmState, init::State as InitState, market::State as MarketState,
    miner::State as MinerState, multisig::State as MultiSigState, power::State as PowerState,
    reward::State as RewardState, system::State as SystemState,
};
use crate::{
    lotus_json::HasLotusJson as _,
    shim::{
        actors::state_load::*,
        address::Address,
        state_tree::{ActorState, StateTree},
    },
};
use ahash::HashMap;
use cid::Cid;
use colored::*;
use fvm_ipld_blockstore::Blockstore;
use ipld_core::ipld::Ipld;
use resolve::resolve_cids_recursive;
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};

#[derive(Serialize, Deserialize)]
struct ActorStateResolved {
    #[serde(with = "crate::lotus_json")]
    code: Cid,
    sequence: u64,
    balance: String,
    #[serde(with = "crate::lotus_json")]
    state: Ipld,
}

fn actor_to_resolved(
    bs: &impl Blockstore,
    actor: &ActorState,
    depth: Option<u64>,
) -> ActorStateResolved {
    let resolved =
        resolve_cids_recursive(bs, &actor.state, depth).unwrap_or(Ipld::Link(actor.state));
    ActorStateResolved {
        state: resolved,
        code: actor.code,
        balance: actor.balance.to_string(),
        sequence: actor.sequence,
    }
}

fn root_to_state_map<BS: Blockstore>(
    bs: &Arc<BS>,
    root: &Cid,
) -> Result<HashMap<Address, ActorState>, anyhow::Error> {
    let mut actors = HashMap::default();
    let state_tree = StateTree::new_from_root(bs.clone(), root)?;
    state_tree.for_each(|addr: Address, actor: &ActorState| {
        actors.insert(addr, actor.clone());
        Ok(())
    })?;

    Ok(actors)
}

/// Tries to resolve state tree actors, if all data exists in store.
/// The actors HAMT is hard to parse in a diff, so this attempts to remedy this.
/// This function will only print the actors that are added, removed, or changed
/// so it can be used on large state trees.
fn try_print_actor_states<BS: Blockstore>(
    bs: &Arc<BS>,
    root: &Cid,
    expected_root: &Cid,
    depth: Option<u64>,
) -> Result<(), anyhow::Error> {
    // For now, resolving to a map, because we need to use go implementation's
    // inefficient caching this would probably be faster in most cases.
    let mut e_state = root_to_state_map(bs, expected_root)?;

    // Compare state with expected
    let state_tree = StateTree::new_from_root(bs.clone(), root)?;

    state_tree.for_each(|addr: Address, actor| {
        let calc_pp = pp_actor_state(bs, actor, depth)?;

        if let Some(other) = e_state.remove(&addr) {
            if &other != actor {
                let comma = ",";
                let expected_pp = pp_actor_state(bs, &other, depth)?;
                let expected = expected_pp
                    .split(comma)
                    .map(|s| s.trim_start_matches('\n'))
                    .collect::<Vec<&str>>();
                let calculated = calc_pp
                    .split(comma)
                    .map(|s| s.trim_start_matches('\n'))
                    .collect::<Vec<&str>>();
                let diffs = TextDiff::from_slices(&expected, &calculated);
                let stdout = stdout();
                let mut handle = stdout.lock();
                writeln!(handle, "Address {addr} changed: ")?;
                print_diffs(&mut handle, diffs)?;
            }
        } else {
            // Added actor, print out the json format actor state.
            println!("{}", format!("+ Address {addr}:\n{calc_pp}").green());
        }

        Ok(())
    })?;

    // Print all addresses that no longer have actor state
    for (addr, state) in e_state.into_iter() {
        let expected_json = serde_json::to_string_pretty(&actor_to_resolved(bs, &state, depth))?;
        println!("{}", format!("- Address {addr}:\n{expected_json}").red())
    }

    Ok(())
}

fn pp_actor_state(
    bs: &impl Blockstore,
    actor_state: &ActorState,
    depth: Option<u64>,
) -> Result<String, anyhow::Error> {
    let mut buffer = String::new();
    writeln!(&mut buffer, "{actor_state:?}")?;
    if let Ok(miner_state) = MinerState::load(bs, actor_state.code, actor_state.state) {
        write!(&mut buffer, "{miner_state:?}")?;
        return Ok(buffer);
    }
    if let Ok(cron_state) = CronState::load(bs, actor_state.code, actor_state.state) {
        write!(&mut buffer, "{cron_state:?}")?;
        return Ok(buffer);
    }
    if let Ok(account_state) = AccountState::load(bs, actor_state.code, actor_state.state) {
        write!(&mut buffer, "{account_state:?}")?;
        return Ok(buffer);
    }
    if let Ok(power_state) = PowerState::load(bs, actor_state.code, actor_state.state) {
        write!(&mut buffer, "{power_state:?}")?;
        return Ok(buffer);
    }
    if let Ok(init_state) = InitState::load(bs, actor_state.code, actor_state.state) {
        write!(&mut buffer, "{init_state:?}")?;
        return Ok(buffer);
    }
    if let Ok(reward_state) = RewardState::load(bs, actor_state.code, actor_state.state) {
        write!(&mut buffer, "{reward_state:?}")?;
        return Ok(buffer);
    }
    if let Ok(system_state) = SystemState::load(bs, actor_state.code, actor_state.state) {
        write!(&mut buffer, "{system_state:?}")?;
        return Ok(buffer);
    }
    if let Ok(multi_sig_state) = MultiSigState::load(bs, actor_state.code, actor_state.state) {
        write!(&mut buffer, "{multi_sig_state:?}")?;
        return Ok(buffer);
    }
    if let Ok(market_state) = MarketState::load(bs, actor_state.code, actor_state.state) {
        write!(&mut buffer, "{market_state:?}")?;
        return Ok(buffer);
    }
    if let Ok(datacap_state) = DatacapState::load(bs, actor_state.code, actor_state.state) {
        write!(&mut buffer, "{datacap_state:?}")?;
        return Ok(buffer);
    }
    if let Ok(evm_state) = EvmState::load(bs, actor_state.code, actor_state.state) {
        write!(&mut buffer, "{evm_state:?}")?;
        return Ok(buffer);
    }

    let resolved = actor_to_resolved(bs, actor_state, depth);
    buffer = serde_json::to_string_pretty(&resolved)?;
    Ok(buffer)
}

fn print_diffs(handle: &mut impl Write, diffs: TextDiff<str>) -> std::io::Result<()> {
    for op in diffs.ops() {
        for change in diffs.iter_changes(op) {
            match change.tag() {
                ChangeTag::Delete => writeln!(handle, "{}", format!("-{}", change.value()).red())?,
                ChangeTag::Insert => {
                    writeln!(handle, "{}", format!("+{}", change.value()).green())?
                }
                ChangeTag::Equal => writeln!(handle, " {}", change.value())?,
            };
        }
    }
    Ok(())
}

/// Prints a diff of the resolved state tree.
/// If the actor's HAMT cannot be loaded, base IPLD resolution is given.
pub fn print_state_diff<BS>(
    bs: &Arc<BS>,
    root: &Cid,
    expected_root: &Cid,
    depth: Option<u64>,
) -> Result<(), anyhow::Error>
where
    BS: Blockstore,
{
    if let Err(e) = try_print_actor_states(bs, root, expected_root, depth) {
        println!("Could not resolve actor states: {e}\nUsing default resolution:");
        let expected = resolve_cids_recursive(bs, expected_root, depth)?;
        let actual = resolve_cids_recursive(bs, root, depth)?;

        let expected_json = expected.into_lotus_json_string_pretty()?;
        let actual_json = actual.into_lotus_json_string_pretty()?;

        let diffs = TextDiff::from_lines(&expected_json, &actual_json);

        let stdout = stdout();
        let mut handle = stdout.lock();
        print_diffs(&mut handle, diffs)?
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::db::MemoryDB;
    use crate::shim::{address::Address, econ::TokenAmount, state_tree::ActorState};
    use crate::utils::db::CborStoreExt;
    use cid::Cid;
    use fil_actor_account_state::v10::State as AccountState;
    use fvm_ipld_blockstore::Blockstore;

    use super::pp_actor_state;

    fn mk_account_v10(db: &impl Blockstore, account: &AccountState) -> ActorState {
        // mainnet v10 account actor cid
        let account_cid =
            Cid::try_from("bafk2bzaceampw4romta75hyz5p4cqriypmpbgnkxncgxgqn6zptv5lsp2w2bo")
                .unwrap();
        let actor_state_cid = db.put_cbor_default(&account).unwrap();
        ActorState::new(
            account_cid,
            actor_state_cid,
            TokenAmount::from_atto(0),
            0,
            None,
        )
    }

    // Account states should be parsed and pretty-printed.
    #[test]
    fn correctly_pretty_print_account_actor_state() {
        let db = MemoryDB::default();

        let account_state = AccountState {
            address: Address::new_id(0xdeadbeef).into(),
        };
        let state = mk_account_v10(&db, &account_state);

        let pretty = pp_actor_state(&db, &state, None).unwrap();

        assert_eq!(
            pretty,
            "ActorState(\
                ActorState { \
                    code: Cid(bafk2bzaceampw4romta75hyz5p4cqriypmpbgnkxncgxgqn6zptv5lsp2w2bo), \
                    state: Cid(bafy2bzaceaiws3hdhmfyxyfjzmbaxv5aw6eywwbipeae4n5jjg5smmfxsaeic), \
                    sequence: 0, balance: TokenAmount(0.0), delegated_address: None })\n\
            V10(State { address: Address { payload: ID(3735928559) } })"
        );
    }

    // When we cannot identify (or parse) an actor state, we should print the IPLD
    // as JSON
    #[test]
    fn check_json_fallback_if_unknown_actor() {
        let db = MemoryDB::default();

        let account_state = AccountState {
            address: Address::new_id(0xdeadbeef).into(),
        };
        let mut state = mk_account_v10(&db, &account_state);
        state.code = Cid::default(); // Use an unknown actor CID to force parsing to fail.

        let pretty = pp_actor_state(&db, &state, None).unwrap();

        assert_eq!(
            pretty,
            "{
  \"code\": {
    \"/\": \"baeaaaaa\"
  },
  \"sequence\": 0,
  \"balance\": \"0.0\",
  \"state\": [
    {
      \"/\": {
        \"bytes\": \"mAO/9tvUN\"
      }
    }
  ]
}"
        );
    }
}

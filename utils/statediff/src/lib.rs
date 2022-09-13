// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod resolve;

// use actor::account;
// use actor::market;
// use actor::miner;
// use actor::power;
use cid::Cid;
use colored::*;
use difference::{Changeset, Difference};
use forest_ipld::json::{IpldJson, IpldJsonRef};
use forest_ipld_blockstore::BlockStore;
use forest_json::cid::CidJson;
use fvm::state_tree::{ActorState, StateTree};
use fvm_shared::address::Address;
use libipld_core::ipld::Ipld;
use resolve::resolve_cids_recursive;
use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::io::stdout;
use std::io::Write;

use fvm_ipld_bitfield::BitField;
use fvm_ipld_encoding::tuple::*;
use fvm_shared::bigint::bigint_ser;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use fvm_shared::sector::StoragePower;
use fvm_shared::smooth::FilterEstimate;

/// State includes the address for the actor
#[derive(Serialize_tuple, Deserialize_tuple, Debug)]
pub struct AccountState {
    pub address: Address,
}

/// Cron actor state which holds entries to call during epoch tick
#[derive(Default, Serialize_tuple, Deserialize_tuple, Debug)]
pub struct CronState {
    /// Entries is a set of actors (and corresponding methods) to call during `EpochTick`.
    pub entries: Vec<CronEntry>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct CronEntry {
    /// The actor to call (ID address)
    pub receiver: Address,
    /// The method number to call (must accept empty parameters)
    pub method_num: fvm_shared::MethodNum,
}

/// Storage power actor state
#[derive(Default, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct PowerState {
    #[serde(with = "bigint_ser")]
    pub total_raw_byte_power: StoragePower,
    #[serde(with = "bigint_ser")]
    pub total_bytes_committed: StoragePower,
    #[serde(with = "bigint_ser")]
    pub total_quality_adj_power: StoragePower,
    #[serde(with = "bigint_ser")]
    pub total_qa_bytes_committed: StoragePower,
    #[serde(with = "bigint_ser")]
    pub total_pledge_collateral: TokenAmount,

    #[serde(with = "bigint_ser")]
    pub this_epoch_raw_byte_power: StoragePower,
    #[serde(with = "bigint_ser")]
    pub this_epoch_quality_adj_power: StoragePower,
    #[serde(with = "bigint_ser")]
    pub this_epoch_pledge_collateral: TokenAmount,
    pub this_epoch_qa_power_smoothed: FilterEstimate,

    pub miner_count: i64,
    /// Number of miners having proven the minimum consensus power.
    pub miner_above_min_power_count: i64,

    /// A queue of events to be triggered by cron, indexed by epoch.
    pub cron_event_queue: Cid, // Multimap, (HAMT[ChainEpoch]AMT[CronEvent]

    /// First epoch in which a cron task may be stored. Cron will iterate every epoch between this
    /// and the current epoch inclusively to find tasks to execute.
    pub first_cron_epoch: ChainEpoch,

    /// Claimed power for each miner.
    pub claims: Cid, // Map, HAMT[address]Claim

    pub proof_validation_batch: Option<Cid>,
}

#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug)]
pub struct MinerState {
    /// Contains static info about this miner
    pub info: Cid,

    /// Total funds locked as `pre_commit_deposit`
    #[serde(with = "bigint_ser")]
    pub pre_commit_deposits: TokenAmount,

    /// Total rewards and added funds locked in vesting table
    #[serde(with = "bigint_ser")]
    pub locked_funds: TokenAmount,

    /// `VestingFunds` (Vesting Funds schedule for the miner).
    pub vesting_funds: Cid,

    /// Absolute value of debt this miner owes from unpaid fees.
    #[serde(with = "bigint_ser")]
    pub fee_debt: TokenAmount,

    /// Sum of initial pledge requirements of all active sectors.
    #[serde(with = "bigint_ser")]
    pub initial_pledge: TokenAmount,

    /// Sectors that have been pre-committed but not yet proven.
    /// `Map, HAMT<SectorNumber, SectorPreCommitOnChainInfo>`
    pub pre_committed_sectors: Cid,

    // PreCommittedSectorsCleanUp maintains the state required to cleanup expired PreCommittedSectors.
    pub pre_committed_sectors_cleanup: Cid, // BitFieldQueue (AMT[Epoch]*BitField)

    /// Allocated sector IDs. Sector IDs can never be reused once allocated.
    pub allocated_sectors: Cid, // BitField

    /// Information for all proven and not-yet-garbage-collected sectors.
    ///
    /// Sectors are removed from this AMT when the partition to which the
    /// sector belongs is compacted.
    pub sectors: Cid, // Array, AMT[SectorNumber]SectorOnChainInfo (sparse)

    /// The first epoch in this miner's current proving period. This is the first epoch in which a PoSt for a
    /// partition at the miner's first deadline may arrive. Alternatively, it is after the last epoch at which
    /// a PoSt for the previous window is valid.
    /// Always greater than zero, this may be greater than the current epoch for genesis miners in the first
    /// `WPoStProvingPeriod` epochs of the chain; the epochs before the first proving period starts are exempt from Window
    /// PoSt requirements.
    /// Updated at the end of every period by a cron callback.
    pub proving_period_start: ChainEpoch,

    /// Index of the deadline within the proving period beginning at `ProvingPeriodStart` that has not yet been
    /// finalized.
    /// Updated at the end of each deadline window by a cron callback.
    pub current_deadline: u64,

    /// The sector numbers due for PoSt at each deadline in the current proving period, frozen at period start.
    /// New sectors are added and expired ones removed at proving period boundary.
    /// Faults are not subtracted from this in state, but on the fly.
    pub deadlines: Cid,

    /// Deadlines with outstanding fees for early sector termination.
    pub early_terminations: BitField,

    // True when miner cron is active, false otherwise
    pub deadline_cron_active: bool,
}

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
/// The actors HAMT is hard to parse in a diff, so this attempts to remedy this.
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
    if let Ok(state) = forest_ipld::from_ipld::<PowerState>(ipld.clone()) {
        write!(&mut buffer, "{:?}", state)?;
        return Ok(buffer);
    }
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
/// If the actor's HAMT cannot be loaded, base IPLD resolution is given.
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

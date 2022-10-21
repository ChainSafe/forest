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

use fvm_ipld_bitfield::BitField;
use fvm_ipld_encoding::tuple::*;
use fvm_shared::bigint::bigint_ser;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::deal::DealID;
use fvm_shared::econ::TokenAmount;
use fvm_shared::sector::{Spacetime, StoragePower};
use fvm_shared::smooth::FilterEstimate;
use fvm_shared::ActorID;

/// State includes the address for the actor
#[derive(Serialize_tuple, Deserialize_tuple, Debug)]
struct AccountState {
    pub address: Address,
}

/// Cron actor state which holds entries to call during epoch tick
#[derive(Default, Serialize_tuple, Deserialize_tuple, Debug)]
struct CronState {
    /// Entries is a set of actors (and corresponding methods) to call during `EpochTick`.
    pub entries: Vec<CronEntry>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize_tuple, Deserialize_tuple)]
struct CronEntry {
    /// The actor to call (ID address)
    pub receiver: Address,
    /// The method number to call (must accept empty parameters)
    pub method_num: fvm_shared::MethodNum,
}

/// Storage power actor state
#[derive(Default, Debug, Serialize_tuple, Deserialize_tuple)]
struct PowerState {
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
struct MinerState {
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

#[derive(Clone, Default, Serialize_tuple, Deserialize_tuple, Debug)]
struct MarketState {
    /// Proposals are deals that have been proposed and not yet cleaned up after expiry or termination.
    /// `Array<DealID, DealProposal>`
    pub proposals: Cid,

    // States contains state for deals that have been activated and not yet cleaned up after expiry or termination.
    // After expiration, the state exists until the proposal is cleaned up too.
    // Invariant: keys(States) ⊆ keys(Proposals).
    /// `Array<DealID, DealState>`
    pub states: Cid,

    /// `PendingProposals` tracks `dealProposals` that have not yet reached their deal start date.
    /// We track them here to ensure that miners can't publish the same deal proposal twice
    pub pending_proposals: Cid,

    /// Total amount held in escrow, indexed by actor address (including both locked and unlocked amounts).
    pub escrow_table: Cid,

    /// Amount locked, indexed by actor address.
    /// Note: the amounts in this table do not affect the overall amount in escrow:
    /// only the _portion_ of the total escrow amount that is locked.
    pub locked_table: Cid,

    /// The next sequential deal id
    pub next_id: DealID,

    /// Metadata cached for efficient iteration over deals.
    /// `SetMultimap<Address>`
    pub deal_ops_by_epoch: Cid,
    pub last_cron: ChainEpoch,

    /// Total Client Collateral that is locked. Unlocked when deal is terminated
    #[serde(with = "bigint_ser")]
    pub total_client_locked_collateral: TokenAmount,
    /// Total Provider Collateral that is locked. Unlocked when deal is terminated
    #[serde(with = "bigint_ser")]
    pub total_provider_locked_collateral: TokenAmount,
    /// Total storage fee that is locked in escrow. Unlocked when payments are made
    #[serde(with = "bigint_ser")]
    pub total_client_storage_fee: TokenAmount,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Hash, Eq, PartialEq, PartialOrd)]
#[serde(transparent)]
struct TxnID(pub i64);

#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug)]
struct MultiSigState {
    pub signers: Vec<Address>,
    pub num_approvals_threshold: u64,
    pub next_tx_id: TxnID,

    // Linear unlock
    #[serde(with = "bigint_ser")]
    pub initial_balance: TokenAmount,
    pub start_epoch: ChainEpoch,
    pub unlock_duration: ChainEpoch,

    pub pending_txs: Cid,
}

#[derive(Default, Deserialize_tuple, Serialize_tuple, Debug)]
struct SystemState {
    // builtin actor registry: Vec<(String, Cid)>
    pub builtin_actors: Cid,
}

#[derive(Serialize_tuple, Deserialize_tuple, Default, Debug)]
struct RewardState {
    /// Target `CumsumRealized` needs to reach for `EffectiveNetworkTime` to increase
    /// Expressed in byte-epochs.
    #[serde(with = "bigint_ser")]
    pub cumsum_baseline: Spacetime,

    /// `CumsumRealized` is cumulative sum of network power capped by `BaselinePower(epoch)`.
    /// Expressed in byte-epochs.
    #[serde(with = "bigint_ser")]
    pub cumsum_realized: Spacetime,

    /// Ceiling of real effective network time `theta` based on
    /// `CumsumBaselinePower(theta) == CumsumRealizedPower`
    /// Theta captures the notion of how much the network has progressed in its baseline
    /// and in advancing network time.
    pub effective_network_time: ChainEpoch,

    /// `EffectiveBaselinePower` is the baseline power at the `EffectiveNetworkTime` epoch.
    #[serde(with = "bigint_ser")]
    pub effective_baseline_power: StoragePower,

    /// The reward to be paid in per `WinCount` to block producers.
    /// The actual reward total paid out depends on the number of winners in any round.
    /// This value is recomputed every non-null epoch and used in the next non-null epoch.
    #[serde(with = "bigint_ser")]
    pub this_epoch_reward: TokenAmount,
    /// Smoothed `this_epoch_reward`.
    pub this_epoch_reward_smoothed: FilterEstimate,

    /// The baseline power the network is targeting at st.Epoch.
    #[serde(with = "bigint_ser")]
    pub this_epoch_baseline_power: StoragePower,

    /// Epoch tracks for which epoch the Reward was computed.
    pub epoch: ChainEpoch,

    // TotalStoragePowerReward tracks the total FIL awarded to block miners
    #[serde(with = "bigint_ser")]
    pub total_storage_power_reward: TokenAmount,

    // Simple and Baseline totals are constants used for computing rewards.
    // They are on chain because of a historical fix resetting baseline value
    // in a way that depended on the history leading immediately up to the
    // migration fixing the value.  These values can be moved from state back
    // into a code constant in a subsequent upgrade.
    #[serde(with = "bigint_ser")]
    pub simple_total: TokenAmount,
    #[serde(with = "bigint_ser")]
    pub baseline_total: TokenAmount,
}

#[derive(Serialize_tuple, Deserialize_tuple, Debug)]
struct InitState {
    pub address_map: Cid,
    pub next_id: ActorID,
    pub network_name: String,
}

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

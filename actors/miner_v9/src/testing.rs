use crate::{
    power_for_sectors, BitFieldQueue, Deadline, ExpirationQueue, MinerInfo, Partition, PowerPair,
    SectorOnChainInfo, SectorPreCommitOnChainInfo, Sectors, State,
};
use fil_actors_runtime_v9::runtime::Policy;
use fil_actors_runtime_v9::{parse_uint_key, Map, MessageAccumulator};
use fvm_ipld_bitfield::BitField;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use fvm_shared::address::Protocol;
use fvm_shared::clock::{ChainEpoch, QuantSpec, NO_QUANTIZATION};
use fvm_shared::deal::DealID;
use fvm_shared::econ::TokenAmount;
use fvm_shared::sector::{RegisteredPoStProof, SectorNumber, SectorSize};
use num_traits::Zero;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub fn check_state_invariants<BS: Blockstore>(
    policy: &Policy,
    state: &State,
    store: &BS,
    balance: &TokenAmount,
) -> (StateSummary, MessageAccumulator) {
    let acc = MessageAccumulator::default();
    let sector_size;

    let mut miner_summary = StateSummary {
        deadline_cron_active: state.deadline_cron_active,
        ..Default::default()
    };

    // load data from linked structures
    match state.get_info(store) {
        Ok(info) => {
            miner_summary.window_post_proof_type = info.window_post_proof_type;
            sector_size = info.sector_size;
            check_miner_info(info, &acc);
        }
        Err(e) => {
            // Stop here, it's too hard to make other useful checks.
            acc.add(format!("error loading miner info: {e}"));
            return (miner_summary, acc);
        }
    };

    check_miner_balances(policy, state, store, balance, &acc);

    let allocated_sectors = match store.get_cbor::<BitField>(&state.allocated_sectors) {
        Ok(Some(allocated_sectors)) => {
            if let Some(sectors) = allocated_sectors.bounded_iter(1 << 30) {
                sectors.map(|i| i as SectorNumber).collect()
            } else {
                acc.add("error expanding allocated sector bitfield");
                BTreeSet::new()
            }
        }
        Ok(None) => {
            acc.add("error loading allocated sector bitfield");
            BTreeSet::new()
        }
        Err(e) => {
            acc.add(format!("error loading allocated sector bitfield: {e}"));
            BTreeSet::new()
        }
    };

    check_precommits(policy, state, store, &allocated_sectors, &acc);

    let mut all_sectors: BTreeMap<SectorNumber, SectorOnChainInfo> = BTreeMap::new();
    match Sectors::load(&store, &state.sectors) {
        Ok(sectors) => {
            let ret = sectors.amt.for_each(|sector_number, sector| {
                all_sectors.insert(sector_number, sector.clone());
                acc.require(
                    allocated_sectors.contains(&sector_number),
                    format!(
                        "on chain sector's sector number has not been allocated {sector_number}"
                    ),
                );
                sector.deal_ids.iter().for_each(|&deal| {
                    miner_summary.deals.insert(
                        deal,
                        DealSummary {
                            sector_start: sector.activation,
                            sector_expiration: sector.expiration,
                        },
                    );
                });
                Ok(())
            });

            acc.require_no_error(ret, "error iterating sectors");
        }
        Err(e) => acc.add(format!("error loading sectors: {e}")),
    };

    // check deadlines
    acc.require(
        state.current_deadline < policy.wpost_period_deadlines,
        format!(
            "current deadline index is greater than deadlines per period({}): {}",
            policy.wpost_period_deadlines, state.current_deadline
        ),
    );

    match state.load_deadlines(store) {
        Ok(deadlines) => {
            let ret = deadlines.for_each(policy, store, |deadline_index, deadline| {
                let acc = acc.with_prefix(format!("deadline {deadline_index}: "));
                let quant = state.quant_spec_for_deadline(policy, deadline_index);
                let deadline_summary = check_deadline_state_invariants(
                    &deadline,
                    store,
                    quant,
                    sector_size,
                    &all_sectors,
                    &acc,
                );

                miner_summary.live_power += &deadline_summary.live_power;
                miner_summary.active_power += &deadline_summary.active_power;
                miner_summary.faulty_power += &deadline_summary.faulty_power;
                Ok(())
            });

            acc.require_no_error(ret, "error iterating deadlines");
        }
        Err(e) => {
            acc.add(format!("error loading deadlines: {e}"));
        }
    };

    (miner_summary, acc)
}

pub struct DealSummary {
    pub sector_start: ChainEpoch,
    pub sector_expiration: ChainEpoch,
}

pub struct StateSummary {
    pub live_power: PowerPair,
    pub active_power: PowerPair,
    pub faulty_power: PowerPair,
    pub deals: BTreeMap<DealID, DealSummary>,
    pub window_post_proof_type: RegisteredPoStProof,
    pub deadline_cron_active: bool,
}

impl Default for StateSummary {
    fn default() -> Self {
        StateSummary {
            live_power: PowerPair::zero(),
            active_power: PowerPair::zero(),
            faulty_power: PowerPair::zero(),
            window_post_proof_type: RegisteredPoStProof::Invalid(0),
            deadline_cron_active: false,
            deals: BTreeMap::new(),
        }
    }
}

fn check_miner_info(info: MinerInfo, acc: &MessageAccumulator) {
    acc.require(
        info.owner.protocol() == Protocol::ID,
        format!("owner address {} is not an ID address", info.owner),
    );
    acc.require(
        info.worker.protocol() == Protocol::ID,
        format!("worker address {} is not an ID address", info.worker),
    );
    info.control_addresses.iter().for_each(|address| {
        acc.require(
            address.protocol() == Protocol::ID,
            format!("control address {} is not an ID address", address),
        )
    });

    if let Some(pending_worker_key) = info.pending_worker_key {
        acc.require(
            pending_worker_key.new_worker.protocol() == Protocol::ID,
            format!(
                "pending worker address {} is not an ID address",
                pending_worker_key.new_worker
            ),
        );
        acc.require(
            pending_worker_key.new_worker != info.worker,
            format!(
                "pending worker key {} is same as existing worker {}",
                pending_worker_key.new_worker, info.worker
            ),
        );
    }

    if let Some(pending_owner_address) = info.pending_owner_address {
        acc.require(
            pending_owner_address.protocol() == Protocol::ID,
            format!(
                "pending owner address {} is not an ID address",
                pending_owner_address
            ),
        );
        acc.require(
            pending_owner_address != info.owner,
            format!(
                "pending owner address {} is same as existing owner {}",
                pending_owner_address, info.owner
            ),
        );
    }

    if let RegisteredPoStProof::Invalid(id) = info.window_post_proof_type {
        acc.add(format!("invalid Window PoSt proof type {id}"));
    } else {
        // safe to unwrap as we know it's valid at this point
        let sector_size = info.window_post_proof_type.sector_size().unwrap();
        acc.require(
            info.sector_size == sector_size,
            format!(
                "sector size {} is wrong for Window PoSt proof type {:?}: {}",
                info.sector_size, info.window_post_proof_type, sector_size
            ),
        );

        let partition_sectors = info
            .window_post_proof_type
            .window_post_partitions_sector()
            .unwrap();
        acc.require(info.window_post_partition_sectors == partition_sectors, format!("miner partition sectors {} does not match partition sectors {} for PoSt proof type {:?}", info.window_post_partition_sectors, partition_sectors, info.window_post_proof_type));
    }
}

fn check_miner_balances<BS: Blockstore>(
    policy: &Policy,
    state: &State,
    store: &BS,
    balance: &TokenAmount,
    acc: &MessageAccumulator,
) {
    acc.require(
        !balance.is_negative(),
        format!("miner actor balance is less than zero: {balance}"),
    );
    acc.require(
        !state.locked_funds.is_negative(),
        format!(
            "miner locked funds is less than zero: {}",
            state.locked_funds
        ),
    );
    acc.require(
        !state.pre_commit_deposits.is_negative(),
        format!(
            "miner precommit deposit is less than zero: {}",
            state.pre_commit_deposits
        ),
    );
    acc.require(
        !state.initial_pledge.is_negative(),
        format!(
            "miner initial pledge is less than zero: {}",
            state.initial_pledge
        ),
    );
    acc.require(
        !state.fee_debt.is_negative(),
        format!("miner fee debt is less than zero: {}", state.fee_debt),
    );

    acc.require(!(balance - &state.locked_funds - &state.pre_commit_deposits - &state.initial_pledge).is_negative(), format!("miner balance {balance} is less than sum of locked funds ({}), precommit deposit ({}) and initial pledge ({})", state.locked_funds, state.pre_commit_deposits, state.initial_pledge));

    // locked funds must be sum of vesting table and vesting table payments must be quantized
    let mut vesting_sum = TokenAmount::zero();
    match state.load_vesting_funds(store) {
        Ok(funds) => {
            let quant = state.quant_spec_every_deadline(policy);
            funds.funds.iter().for_each(|entry| {
                acc.require(
                    entry.amount.is_positive(),
                    format!("non-positive amount in miner vesting table entry {entry:?}"),
                );
                vesting_sum += &entry.amount;

                let quantized = quant.quantize_up(entry.epoch);
                acc.require(
                    entry.epoch == quantized,
                    format!(
                        "vesting table entry has non-quantized epoch {} (should be {quantized})",
                        entry.epoch
                    ),
                );
            });
        }
        Err(e) => {
            acc.add(format!("error loading vesting funds: {e}"));
        }
    };

    acc.require(
        state.locked_funds == vesting_sum,
        format!(
            "locked funds {} is not sum of vesting table entries {vesting_sum}",
            state.locked_funds
        ),
    );

    // non zero funds implies that DeadlineCronActive is true
    if state.continue_deadline_cron() {
        acc.require(
            state.deadline_cron_active,
            "DeadlineCronActive == false when IP+PCD+LF > 0",
        );
    }
}

fn check_precommits<BS: Blockstore>(
    policy: &Policy,
    state: &State,
    store: &BS,
    allocated_sectors: &BTreeSet<u64>,
    acc: &MessageAccumulator,
) {
    let quant = state.quant_spec_every_deadline(policy);

    // invert pre-commit clean up queue into a lookup by sector number
    let mut cleanup_epochs: BTreeMap<u64, ChainEpoch> = BTreeMap::new();
    match BitFieldQueue::new(store, &state.pre_committed_sectors_cleanup, quant) {
        Ok(queue) => {
            let ret = queue.amt.for_each(|epoch, expiration_bitfield| {
                let epoch = epoch as ChainEpoch;
                let quantized = quant.quantize_up(epoch);
                acc.require(
                    quantized == epoch,
                    format!("pre-commit expiration {epoch} is not quantized"),
                );

                expiration_bitfield.iter().for_each(|sector_number| {
                    cleanup_epochs.insert(sector_number, epoch);
                });
                Ok(())
            });
            acc.require_no_error(ret, "error iterating pre-commit clean-up queue");
        }
        Err(e) => {
            acc.add(format!("error loading pre-commit clean-up queue: {e}"));
        }
    };

    let mut precommit_total = TokenAmount::zero();

    let precommited_sectors =
        Map::<_, SectorPreCommitOnChainInfo>::load(&state.pre_committed_sectors, store);

    match precommited_sectors {
        Ok(precommited_sectors) => {
            let ret = precommited_sectors.for_each(|key, precommit| {
                let sector_number = match parse_uint_key(key) {
                    Ok(sector_number) => sector_number,
                    Err(e) => {
                        acc.add(format!("error parsing pre-commit key as uint: {e}"));
                        return Ok(());
                    }
                };

                acc.require(
                    allocated_sectors.contains(&sector_number),
                    format!("pre-commited sector number has not been allocated {sector_number}"),
                );

                acc.require(
                    cleanup_epochs.contains_key(&sector_number),
                    format!(
                        "no clean-up epoch for pre-commit at {}",
                        precommit.pre_commit_epoch
                    ),
                );
                precommit_total += &precommit.pre_commit_deposit;
                Ok(())
            });
            acc.require_no_error(ret, "error iterating pre-commited sectors");
        }
        Err(e) => {
            acc.add(format!("error loading precommited_sectors: {e}"));
        }
    };

    acc.require(state.pre_commit_deposits == precommit_total,format!("sum of pre-commit deposits {precommit_total} does not equal recorded pre-commit deposit {}", state.pre_commit_deposits));
}

#[derive(Default)]
pub struct DeadlineStateSummary {
    pub all_sectors: BitField,
    pub live_sectors: BitField,
    pub faulty_sectors: BitField,
    pub recovering_sectors: BitField,
    pub unproven_sectors: BitField,
    pub terminated_sectors: BitField,
    pub live_power: PowerPair,
    pub active_power: PowerPair,
    pub faulty_power: PowerPair,
}

pub type SectorsMap = BTreeMap<SectorNumber, SectorOnChainInfo>;

#[derive(Default)]
pub struct PartitionStateSummary {
    pub all_sectors: BitField,
    pub live_sectors: BitField,
    pub faulty_sectors: BitField,
    pub recovering_sectors: BitField,
    pub unproven_sectors: BitField,
    pub terminated_sectors: BitField,
    pub live_power: PowerPair,
    pub active_power: PowerPair,
    pub faulty_power: PowerPair,
    pub recovering_power: PowerPair,
    // Epochs at which some sector is scheduled to expire.
    pub expiration_epochs: Vec<ChainEpoch>,
    pub early_termination_count: usize,
}

impl PartitionStateSummary {
    pub fn check_partition_state_invariants<BS: Blockstore>(
        partition: &Partition,
        store: &BS,
        quant: QuantSpec,
        sector_size: SectorSize,
        sectors_map: &SectorsMap,
        acc: &MessageAccumulator,
    ) -> Self {
        let live = partition.live_sectors();
        let active = partition.active_sectors();

        // live contains all live sectors
        require_contains_all(&live, &active, acc, "live does not contain active");

        // Live contains all faults.
        require_contains_all(
            &live,
            &partition.faults,
            acc,
            "live does not contain faults",
        );

        // Live contains all unproven.
        require_contains_all(
            &live,
            &partition.unproven,
            acc,
            "live does not contain unproven",
        );

        // Active contains no faults
        require_contains_none(&active, &partition.faults, acc, "active includes faults");

        // Active contains no unproven
        require_contains_none(
            &active,
            &partition.unproven,
            acc,
            "active includes unproven",
        );

        // Faults contains all recoveries.
        require_contains_all(
            &partition.faults,
            &partition.recoveries,
            acc,
            "faults do not contain recoveries",
        );

        // Live contains no terminated sectors
        require_contains_none(
            &live,
            &partition.terminated,
            acc,
            "live includes terminations",
        );

        // Unproven contains no faults
        require_contains_none(
            &partition.faults,
            &partition.unproven,
            acc,
            "unproven includes faults",
        );

        // All terminated sectors are part of the partition.
        require_contains_all(
            &partition.sectors,
            &partition.terminated,
            acc,
            "sectors do not contain terminations",
        );

        // Validate power
        let mut live_power = PowerPair::zero();
        let mut faulty_power = PowerPair::zero();
        let mut unproven_power = PowerPair::zero();

        let (live_sectors, missing) = select_sectors_map(sectors_map, &live);
        if missing.is_empty() {
            live_power = power_for_sectors(
                sector_size,
                &live_sectors.values().cloned().collect::<Vec<_>>(),
            );
            acc.require(
                partition.live_power == live_power,
                format!(
                    "live power was {:?}, expected {:?}",
                    partition.live_power, live_power
                ),
            );
        } else {
            acc.add(format!(
                "live sectors missing from all sectors: {missing:?}"
            ));
        }

        let (unproven_sectors, missing) = select_sectors_map(sectors_map, &partition.unproven);
        if missing.is_empty() {
            unproven_power = power_for_sectors(
                sector_size,
                &unproven_sectors.values().cloned().collect::<Vec<_>>(),
            );
            acc.require(
                partition.unproven_power == unproven_power,
                format!(
                    "unproven power power was {:?}, expected {:?}",
                    partition.unproven_power, unproven_power
                ),
            );
        } else {
            acc.add(format!(
                "unproven sectors missing from all sectors: {missing:?}"
            ));
        }

        let (faulty_sectors, missing) = select_sectors_map(sectors_map, &partition.faults);
        if missing.is_empty() {
            faulty_power = power_for_sectors(
                sector_size,
                &faulty_sectors.values().cloned().collect::<Vec<_>>(),
            );
            acc.require(
                partition.faulty_power == faulty_power,
                format!(
                    "faulty power power was {:?}, expected {:?}",
                    partition.faulty_power, faulty_power
                ),
            );
        } else {
            acc.add(format!(
                "faulty sectors missing from all sectors: {missing:?}"
            ));
        }

        let (recovering_sectors, missing) = select_sectors_map(sectors_map, &partition.recoveries);
        if missing.is_empty() {
            let recovering_power = power_for_sectors(
                sector_size,
                &recovering_sectors.values().cloned().collect::<Vec<_>>(),
            );
            acc.require(
                partition.recovering_power == recovering_power,
                format!(
                    "recovering power power was {:?}, expected {:?}",
                    partition.recovering_power, recovering_power
                ),
            );
        } else {
            acc.add(format!(
                "recovering sectors missing from all sectors: {missing:?}"
            ));
        }

        let active_power = &live_power - &faulty_power - unproven_power;
        let partition_active_power = partition.active_power();
        acc.require(
            partition_active_power == active_power,
            format!(
                "active power was {active_power:?}, expected {:?}",
                partition_active_power
            ),
        );

        // validate the expiration queue
        let mut expiration_epochs = Vec::new();
        match ExpirationQueue::new(store, &partition.expirations_epochs, quant) {
            Ok(expiration_queue) => {
                let queue_summary = ExpirationQueueStateSummary::check_expiration_queue(
                    &expiration_queue,
                    &live_sectors,
                    &partition.faults,
                    quant,
                    sector_size,
                    acc,
                );

                expiration_epochs = queue_summary.expiration_epochs;
                // check the queue is compatible with partition fields
                let queue_sectors =
                    BitField::union([&queue_summary.on_time_sectors, &queue_summary.early_sectors]);
                require_equal(
                    &live,
                    &queue_sectors,
                    acc,
                    "live does not equal all expirations",
                );
            }
            Err(err) => {
                acc.add(format!("error loading expiration_queue: {err}"));
            }
        };

        // validate the early termination queue
        let early_termination_count =
            match BitFieldQueue::new(store, &partition.early_terminated, NO_QUANTIZATION) {
                Ok(queue) => check_early_termination_queue(queue, &partition.terminated, acc),
                Err(err) => {
                    acc.add(format!("error loading early termination queue: {err}"));
                    0
                }
            };

        let partition = partition.clone();
        PartitionStateSummary {
            all_sectors: partition.sectors,
            live_sectors: live,
            faulty_sectors: partition.faults,
            recovering_sectors: partition.recoveries,
            unproven_sectors: partition.unproven,
            terminated_sectors: partition.terminated,
            live_power,
            active_power,
            faulty_power: partition.faulty_power,
            recovering_power: partition.recovering_power,
            expiration_epochs,
            early_termination_count,
        }
    }
}

#[derive(Default)]
struct ExpirationQueueStateSummary {
    pub on_time_sectors: BitField,
    pub early_sectors: BitField,
    #[allow(dead_code)]
    pub active_power: PowerPair,
    #[allow(dead_code)]
    pub faulty_power: PowerPair,
    #[allow(dead_code)]
    pub on_time_pledge: TokenAmount,
    pub expiration_epochs: Vec<ChainEpoch>,
}

impl ExpirationQueueStateSummary {
    // Checks the expiration queue for consistency.
    fn check_expiration_queue<BS: Blockstore>(
        expiration_queue: &ExpirationQueue<BS>,
        live_sectors: &SectorsMap,
        partition_faults: &BitField,
        quant: QuantSpec,
        sector_size: SectorSize,
        acc: &MessageAccumulator,
    ) -> Self {
        let mut seen_sectors: HashSet<SectorNumber> = HashSet::new();
        let mut all_on_time: Vec<BitField> = Vec::new();
        let mut all_early: Vec<BitField> = Vec::new();
        let mut expiration_epochs: Vec<ChainEpoch> = Vec::new();
        let mut all_active_power = PowerPair::zero();
        let mut all_faulty_power = PowerPair::zero();
        let mut all_on_time_pledge = TokenAmount::zero();

        let ret = expiration_queue.amt.for_each(|epoch, expiration_set| {
            let epoch = epoch as i64;
            let acc = acc.with_prefix(format!("expiration epoch {epoch}: "));
            let quant_up = quant.quantize_up(epoch);
            acc.require(quant_up == epoch, format!("expiration queue key {epoch} is not quantized, expected {quant_up}"));

            expiration_epochs.push(epoch);

            let mut on_time_sectors_pledge = TokenAmount::zero();
            for sector_number in expiration_set.on_time_sectors.iter() {
                // check sectors are present only once
                if !seen_sectors.insert(sector_number) {
                    acc.add(format!("sector {sector_number} in expiration queue twice"));
                }

                // check expiring sectors are still alive
                if let Some(sector) = live_sectors.get(&sector_number) {
                    let target = quant.quantize_up(sector.expiration);
                    acc.require(epoch == target, format!("invalid expiration {epoch} for sector {sector_number}, expected {target}"));
                    on_time_sectors_pledge += sector.initial_pledge.clone();
                } else {
                    acc.add(format!("on time expiration sector {sector_number} isn't live"));
                }
            }

            for sector_number in expiration_set.early_sectors.iter() {
                // check sectors are present only once
                if !seen_sectors.insert(sector_number) {
                    acc.add(format!("sector {sector_number} in expiration queue twice"));
                }

                // check early sectors are faulty
                acc.require(partition_faults.get(sector_number), format!("sector {sector_number} expiring early but not faulty"));

                // check expiring sectors are still alive
                if let Some(sector) = live_sectors.get(&sector_number) {
                    let target = quant.quantize_up(sector.expiration);
                    acc.require(epoch < target, format!("invalid early expiration {epoch} for sector {sector_number}, expected < {target}"));
                } else {
                    acc.add(format!("on time expiration sector {sector_number} isn't live"));
                }
            }


            // validate power and pledge
            let all = BitField::union([&expiration_set.on_time_sectors, &expiration_set.early_sectors]);
            let all_active = &all - partition_faults;
            let (active_sectors, missing) = select_sectors_map(live_sectors, &all_active);
            acc.require(missing.is_empty(), format!("active sectors missing from live: {missing:?}"));

            let all_faulty = &all & partition_faults;
            let (faulty_sectors, missing) = select_sectors_map(live_sectors, &all_faulty);
            acc.require(missing.is_empty(), format!("faulty sectors missing from live: {missing:?}"));

            let active_sectors_power = power_for_sectors(sector_size, &active_sectors.values().cloned().collect::<Vec<_>>());
            acc.require(expiration_set.active_power == active_sectors_power, format!("active power recorded {:?} doesn't match computed {active_sectors_power:?}", expiration_set.active_power));

            let faulty_sectors_power = power_for_sectors(sector_size, &faulty_sectors.values().cloned().collect::<Vec<_>>());
            acc.require(expiration_set.faulty_power == faulty_sectors_power, format!("faulty power recorded {:?} doesn't match computed {faulty_sectors_power:?}", expiration_set.faulty_power));

            acc.require(expiration_set.on_time_pledge == on_time_sectors_pledge, format!("on time pledge recorded {} doesn't match computed: {on_time_sectors_pledge}", expiration_set.on_time_pledge));

            all_on_time.push(expiration_set.on_time_sectors.clone());
            all_early.push(expiration_set.early_sectors.clone());
            all_active_power += &expiration_set.active_power;
            all_faulty_power += &expiration_set.faulty_power;
            all_on_time_pledge += &expiration_set.on_time_pledge;

            Ok(())
        });
        acc.require_no_error(ret, "error iterating early termination bitfield");

        let union_on_time = BitField::union(&all_on_time);
        let union_early = BitField::union(&all_early);

        Self {
            on_time_sectors: union_on_time,
            early_sectors: union_early,
            active_power: all_active_power,
            faulty_power: all_faulty_power,
            on_time_pledge: all_on_time_pledge,
            expiration_epochs,
        }
    }
}

// Checks the early termination queue for consistency.
// Returns the number of sectors in the queue.
fn check_early_termination_queue<BS: Blockstore>(
    early_queue: BitFieldQueue<BS>,
    terminated: &BitField,
    acc: &MessageAccumulator,
) -> usize {
    let mut seen: HashSet<u64> = HashSet::new();
    let mut seen_bitfield = BitField::new();

    let iter_result = early_queue.amt.for_each(|epoch, bitfield| {
        let acc = acc.with_prefix(format!("early termination epoch {epoch}: "));
        for i in bitfield.iter() {
            acc.require(
                !seen.contains(&i),
                format!("sector {i} in early termination queue twice"),
            );
            seen.insert(i);
            seen_bitfield.set(i);
        }
        Ok(())
    });

    acc.require_no_error(iter_result, "error iterating early termination bitfield");
    require_contains_all(
        terminated,
        &seen_bitfield,
        acc,
        "terminated sectors missing early termination entry",
    );

    seen.len()
}

// Selects a subset of sectors from a map by sector number.
// Returns the selected sectors, and a slice of any sector numbers not found.
fn select_sectors_map(sectors: &SectorsMap, include: &BitField) -> (SectorsMap, Vec<SectorNumber>) {
    let mut included = SectorsMap::new();
    let mut missing = Vec::new();

    for n in include.iter() {
        if let Some(sector) = sectors.get(&n) {
            included.insert(n, sector.clone());
        } else {
            missing.push(n);
        }
    }

    (included, missing)
}

fn require_contains_all(
    superset: &BitField,
    subset: &BitField,
    acc: &MessageAccumulator,
    error_msg: &str,
) {
    if !superset.contains_all(subset) {
        acc.add(format!("{error_msg}: {subset:?}, {superset:?}"));
    }
}

fn require_contains_none(
    superset: &BitField,
    subset: &BitField,
    acc: &MessageAccumulator,
    error_msg: &str,
) {
    if superset.contains_any(subset) {
        acc.add(format!("{error_msg}: {subset:?}, {superset:?}"));
    }
}

fn require_equal(first: &BitField, second: &BitField, acc: &MessageAccumulator, msg: &str) {
    require_contains_all(first, second, acc, msg);
    require_contains_all(second, first, acc, msg);
}

pub fn check_deadline_state_invariants<BS: Blockstore>(
    deadline: &Deadline,
    store: &BS,
    quant: QuantSpec,
    sector_size: SectorSize,
    sectors: &SectorsMap,
    acc: &MessageAccumulator,
) -> DeadlineStateSummary {
    // load linked structures
    let partitions = match deadline.partitions_amt(store) {
        Ok(partitions) => partitions,
        Err(e) => {
            // Hard to do any useful checks.
            acc.add(format!("error loading partitions: {e}"));
            return DeadlineStateSummary::default();
        }
    };

    let mut all_sectors = BitField::new();
    let mut all_live_sectors: Vec<BitField> = Vec::new();
    let mut all_faulty_sectors: Vec<BitField> = Vec::new();
    let mut all_recovering_sectors: Vec<BitField> = Vec::new();
    let mut all_unproven_sectors: Vec<BitField> = Vec::new();
    let mut all_terminated_sectors: Vec<BitField> = Vec::new();
    let mut all_live_power = PowerPair::zero();
    let mut all_active_power = PowerPair::zero();
    let mut all_faulty_power = PowerPair::zero();

    let mut partition_count = 0;

    // check partitions
    let mut partitions_with_expirations: HashMap<ChainEpoch, Vec<u64>> = HashMap::new();
    let mut partitions_with_early_terminations = BitField::new();
    partitions
        .for_each(|index, partition| {
            // check sequential partitions
            acc.require(
                index == partition_count,
                format!(
                    "Non-sequential partitions, expected index {partition_count}, found {index}"
                ),
            );
            partition_count += 1;

            let acc = acc.with_prefix(format!("partition {index}"));
            let summary = PartitionStateSummary::check_partition_state_invariants(
                partition,
                store,
                quant,
                sector_size,
                sectors,
                &acc,
            );

            acc.require(
                !all_sectors.contains_any(&summary.all_sectors),
                format!("duplicate sector in partition {index}"),
            );

            summary.expiration_epochs.iter().for_each(|&epoch| {
                partitions_with_expirations
                    .entry(epoch)
                    .or_insert(Vec::new())
                    .push(index);
            });

            if summary.early_termination_count > 0 {
                partitions_with_early_terminations.set(index);
            }

            all_sectors = BitField::union([&all_sectors, &summary.all_sectors]);
            all_live_sectors.push(summary.live_sectors);
            all_faulty_sectors.push(summary.faulty_sectors);
            all_recovering_sectors.push(summary.recovering_sectors);
            all_unproven_sectors.push(summary.unproven_sectors);
            all_terminated_sectors.push(summary.terminated_sectors);
            all_live_power += &summary.live_power;
            all_active_power += &summary.active_power;
            all_faulty_power += &summary.faulty_power;

            Ok(())
        })
        .expect("error iterating partitions");

    // Check invariants on partitions proven
    if let Some(last_proof) = deadline.partitions_posted.last() {
        acc.require(
            partition_count > last_proof,
            format!(
                "expected at least {} partitions, found {partition_count}",
                last_proof + 1
            ),
        );
        acc.require(
            deadline.live_sectors > 0,
            "expected at least one live sector when partitions have been proven",
        );
    }

    // Check partitions snapshot to make sure we take the snapshot after
    // dealing with recovering power and unproven power.
    match deadline.partitions_snapshot_amt(store) {
        Ok(partition_snapshot) => {
            let ret = partition_snapshot.for_each(|i, partition| {
                let acc = acc.with_prefix(format!("partition snapshot {i}"));
                acc.require(
                    partition.recovering_power.is_zero(),
                    "snapshot partition has recovering power",
                );
                acc.require(
                    partition.recoveries.is_empty(),
                    "snapshot partition has pending recoveries",
                );
                acc.require(
                    partition.unproven_power.is_zero(),
                    "snapshot partition has unproven power",
                );
                acc.require(
                    partition.unproven.is_empty(),
                    "snapshot partition has unproven sectors",
                );

                Ok(())
            });
            acc.require_no_error(ret, "error iterating partitions snapshot");
        }
        Err(e) => acc.add(format!("error loading partitions snapshot: {e}")),
    };

    // Check that we don't have any proofs proving partitions that are not in the snapshot.
    match deadline.optimistic_proofs_snapshot_amt(store) {
        Ok(proofs_snapshot) => {
            if let Ok(partitions_snapshot) = deadline.partitions_snapshot_amt(store) {
                let ret = proofs_snapshot.for_each(|_, proof| {
                    for partition in proof.partitions.iter() {
                        match partitions_snapshot.get(partition) {
                            Ok(snapshot) => acc.require(
                                snapshot.is_some(),
                                format!("failed to find partition {partition} for recorded proof in the snapshot"),
                            ),
                            Err(e) => acc.add(format!("error loading partition snapshot: {e}")),
                        }
                    }
                    Ok(())
                });
                acc.require_no_error(ret, "error iterating proofs snapshot");
            }
        }
        Err(e) => acc.add(format!("error loading proofs snapshot: {e}")),
    };

    // check memoized sector and power values
    let live_sectors = BitField::union(&all_live_sectors);
    acc.require(
        deadline.live_sectors == live_sectors.len(),
        format!(
            "deadline live sectors {} != partitions count {}",
            deadline.live_sectors,
            live_sectors.len()
        ),
    );

    acc.require(
        deadline.total_sectors == all_sectors.len(),
        format!(
            "deadline total sectors {} != partitions count {}",
            deadline.total_sectors,
            all_sectors.len()
        ),
    );

    let faulty_sectors = BitField::union(&all_faulty_sectors);
    let recovering_sectors = BitField::union(&all_recovering_sectors);
    let unproven_sectors = BitField::union(&all_unproven_sectors);
    let terminated_sectors = BitField::union(&all_terminated_sectors);

    acc.require(
        deadline.faulty_power == all_faulty_power,
        format!(
            "deadline faulty power {:?} != partitions total {all_faulty_power:?}",
            deadline.faulty_power
        ),
    );

    // Validate partition expiration queue contains an entry for each partition and epoch with an expiration.
    // The queue may be a superset of the partitions that have expirations because we never remove from it.
    match BitFieldQueue::new(store, &deadline.expirations_epochs, quant) {
        Ok(expiration_queue) => {
            for (epoch, expiring_idx) in partitions_with_expirations {
                match expiration_queue.amt.get(epoch as u64) {
                    Ok(expiration_bitfield) if expiration_bitfield.is_some() => {
                        for partition in expiring_idx {
                            acc.require(expiration_bitfield.unwrap().get(partition), format!("expected partition {partition} to be present in deadline expiration queue at epoch {epoch}"));
                        }
                    }
                    Ok(_) => acc.add(format!(
                        "expected to find partition expiration entry at epoch {epoch}"
                    )),
                    Err(e) => acc.add(format!("error fetching expiration bitfield: {e}")),
                }
            }
        }
        Err(e) => acc.add(format!("error loading expiration queue: {e}")),
    }

    // Validate the early termination queue contains exactly the partitions with early terminations.
    require_equal(
        &partitions_with_early_terminations,
        &deadline.early_terminations,
        acc,
        "deadline early terminations doesn't match expected partitions",
    );

    DeadlineStateSummary {
        all_sectors,
        live_sectors,
        faulty_sectors,
        recovering_sectors,
        unproven_sectors,
        terminated_sectors,
        live_power: all_live_power,
        active_power: all_active_power,
        faulty_power: all_faulty_power,
    }
}

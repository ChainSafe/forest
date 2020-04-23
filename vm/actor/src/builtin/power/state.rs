// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::CONSENSUS_MINER_MIN_POWER;
use crate::{BalanceTable, BytesKey, Multimap, Set, StoragePower, HAMT_BIT_WIDTH};
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use encoding::Cbor;
use ipld_blockstore::BlockStore;
use ipld_hamt::Hamt;
use num_bigint::biguint_ser::{BigUintDe, BigUintSer};
use num_traits::{CheckedSub, Zero};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::{Serialized, TokenAmount};

/// Storage power actor state
#[derive(Default)]
pub struct State {
    pub total_network_power: StoragePower,
    pub miner_count: i64,
    /// The balances of pledge collateral for each miner actually held by this actor.
    /// The sum of the values here should always equal the actor's balance.
    /// See Claim for the pledge *requirements* for each actor.
    pub escrow_table: Cid, // BalanceTable (HAMT[address]TokenAmount)

    /// A queue of events to be triggered by cron, indexed by epoch.
    pub cron_event_queue: Cid, // Multimap, (HAMT[ChainEpoch]AMT[CronEvent]

    /// Last chain epoch OnEpochTickEnd was called on
    pub last_epoch_tick: ChainEpoch,

    /// Miners having failed to prove storage.
    pub post_detected_fault_miners: Cid, // Set, HAMT[addr.Address]struct{}

    /// Claimed power and associated pledge requirements for each miner.
    pub claims: Cid, // Map, HAMT[address]Claim

    /// Number of miners having proven the minimum consensus power.
    // TODO: revisit todo in specs-actors
    pub num_miners_meeting_min_power: i64,
}

impl State {
    pub fn new(empty_map_cid: Cid) -> State {
        State {
            escrow_table: empty_map_cid.clone(),
            cron_event_queue: empty_map_cid.clone(),
            post_detected_fault_miners: empty_map_cid.clone(),
            claims: empty_map_cid,
            ..Default::default()
        }
    }

    /// Get miner balance from address using escrow table
    #[allow(dead_code)]
    pub(super) fn get_miner_balance<BS: BlockStore>(
        &self,
        store: &BS,
        miner: &Address,
    ) -> Result<TokenAmount, String> {
        let bt = BalanceTable::from_root(store, &self.escrow_table)?;
        bt.get(miner)
    }

    /// Sets miner balance at address using escrow table
    #[allow(dead_code)]
    pub(super) fn set_miner_balance<BS: BlockStore>(
        &mut self,
        store: &BS,
        miner: &Address,
        amount: TokenAmount,
    ) -> Result<(), String> {
        let mut bt = BalanceTable::from_root(store, &self.escrow_table)?;
        bt.set(miner, amount)?;
        self.escrow_table = bt.root()?;
        Ok(())
    }

    /// Adds amount to miner balance at address using escrow table
    pub(super) fn add_miner_balance<BS: BlockStore>(
        &mut self,
        store: &BS,
        miner: &Address,
        amount: &TokenAmount,
    ) -> Result<(), String> {
        let mut bt = BalanceTable::from_root(store, &self.escrow_table)?;
        bt.add(miner, amount)?;
        self.escrow_table = bt.root()?;
        Ok(())
    }

    /// Subtracts amount to miner balance at address using escrow table
    #[allow(dead_code)]
    pub(super) fn subtract_miner_balance<BS: BlockStore>(
        &mut self,
        store: &BS,
        miner: &Address,
        amount: &TokenAmount,
        balance_floor: &TokenAmount,
    ) -> Result<TokenAmount, String> {
        let mut bt = BalanceTable::from_root(store, &self.escrow_table)?;
        let amount = bt.subtract_with_minimum(miner, amount, balance_floor)?;
        self.escrow_table = bt.root()?;
        Ok(amount)
    }
    pub fn subtract_from_claim<BS: BlockStore>(
        &mut self,
        store: &BS,
        miner: &Address,
        power: &StoragePower,
        pledge: &TokenAmount,
    ) -> Result<(), String> {
        let mut claim = self
            .get_claim(store, miner)?
            .ok_or(format!("no claim for actor {}", miner))?;

        let old_nominal_power = self.compute_nominal_power(store, miner, &claim.power)?;

        claim.power -= power;
        claim.pledge = claim
            .pledge
            .checked_sub(pledge)
            .ok_or("negative claimed pledge")?;

        self.update_claim(store, miner, power, claim, old_nominal_power)
    }
    pub fn add_to_claim<BS: BlockStore>(
        &mut self,
        store: &BS,
        miner: &Address,
        power: &StoragePower,
        pledge: &TokenAmount,
    ) -> Result<(), String> {
        let mut claim = self
            .get_claim(store, miner)?
            .ok_or(format!("no claim for actor {}", miner))?;

        let old_nominal_power = self.compute_nominal_power(store, miner, &claim.power)?;

        claim.power += power;
        claim.pledge += pledge;

        self.update_claim(store, miner, power, claim, old_nominal_power)
    }
    /// Function will update the claim after `add_to_claim` or `subtract_from_claim` are called
    /// * Logic is modified from spec to not use negative values
    /// TODO revisit: logic for parts of this function seem wrong/ unnecessary
    fn update_claim<BS: BlockStore>(
        &mut self,
        store: &BS,
        miner: &Address,
        power: &StoragePower,
        claim: Claim,
        old_nominal_power: StoragePower,
    ) -> Result<(), String> {
        let new_nominal_power = self.compute_nominal_power(store, miner, &claim.power)?;

        let min_power_ref: &StoragePower = &CONSENSUS_MINER_MIN_POWER;
        let prev_below: bool = &old_nominal_power < min_power_ref;
        let still_below: bool = &new_nominal_power < min_power_ref;

        let faulty = self.has_detected_fault(store, miner)?;

        if !faulty {
            if prev_below && !still_below {
                // Just passed min miner size
                self.num_miners_meeting_min_power += 1;
                self.total_network_power += new_nominal_power;
            } else if !prev_below && still_below {
                // just went below min miner size
                self.num_miners_meeting_min_power -= 1;
                self.total_network_power = self
                    .total_network_power
                    .checked_sub(&old_nominal_power)
                    .ok_or("Negative nominal power")?;
            } else if !prev_below && !still_below {
                // Was above the threshold, still above
                self.total_network_power += power;
            }
        }

        if self.num_miners_meeting_min_power < 0 {
            return Err(format!(
                "negative number of miners: {}",
                self.num_miners_meeting_min_power
            ));
        }

        self.set_claim(store, miner, claim)
    }

    /// Gets claim from claims map by address
    pub fn get_claim<BS: BlockStore>(
        &self,
        store: &BS,
        a: &Address,
    ) -> Result<Option<Claim>, String> {
        let map: Hamt<BytesKey, _> =
            Hamt::load_with_bit_width(&self.claims, store, HAMT_BIT_WIDTH)?;

        Ok(map.get(&a.to_bytes())?)
    }

    pub(super) fn set_claim<BS: BlockStore>(
        &mut self,
        store: &BS,
        addr: &Address,
        claim: Claim,
    ) -> Result<(), String> {
        let mut map: Hamt<BytesKey, _> =
            Hamt::load_with_bit_width(&self.claims, store, HAMT_BIT_WIDTH)?;

        map.set(addr.to_bytes().into(), claim)?;
        self.claims = map.flush()?;
        Ok(())
    }

    pub(super) fn delete_claim<BS: BlockStore>(
        &mut self,
        store: &BS,
        addr: &Address,
    ) -> Result<(), String> {
        let mut map: Hamt<BytesKey, _> =
            Hamt::load_with_bit_width(&self.claims, store, HAMT_BIT_WIDTH)?;

        map.delete(&addr.to_bytes())?;
        self.claims = map.flush()?;
        Ok(())
    }

    fn compute_nominal_power<BS: BlockStore>(
        &self,
        store: &BS,
        miner: &Address,
        claimed_power: &StoragePower,
    ) -> Result<StoragePower, String> {
        // Compute nominal power: i.e., the power we infer the miner to have (based on the network's
        // PoSt queries), which may not be the same as the claimed power.
        // Currently, the nominal power may differ from claimed power because of
        // detected faults.
        let found = self.has_detected_fault(store, miner)?;
        if found {
            Ok(StoragePower::zero())
        } else {
            Ok(claimed_power.clone())
        }
    }

    pub(super) fn has_detected_fault<BS: BlockStore>(
        &self,
        store: &BS,
        a: &Address,
    ) -> Result<bool, String> {
        let faulty = Set::from_root(store, &self.post_detected_fault_miners)?;
        Ok(faulty.has(&a.to_bytes())?)
    }

    pub(super) fn put_detected_fault<BS: BlockStore>(
        &mut self,
        s: &BS,
        a: &Address,
    ) -> Result<(), String> {
        let claim = self
            .get_claim(s, a)?
            .ok_or(format!("no claim for actor: {}", a))?;

        let nominal_power = self.compute_nominal_power(s, a, &claim.power)?;
        if nominal_power >= *CONSENSUS_MINER_MIN_POWER {
            self.num_miners_meeting_min_power -= 1;
        }

        let mut faulty_miners = Set::from_root(s, &self.post_detected_fault_miners)?;
        faulty_miners.put(a.to_bytes().into())?;
        self.post_detected_fault_miners = faulty_miners.root()?;

        Ok(())
    }

    pub(super) fn delete_detected_fault<BS: BlockStore>(
        &mut self,
        s: &BS,
        a: &Address,
    ) -> Result<(), String> {
        let mut faulty_miners = Set::from_root(s, &self.post_detected_fault_miners)?;
        faulty_miners.delete(&a.to_bytes())?;
        self.post_detected_fault_miners = faulty_miners.root()?;

        let claim = self
            .get_claim(s, a)?
            .ok_or(format!("no claim for actor: {}", a))?;

        let nominal_power = self.compute_nominal_power(s, a, &claim.power)?;
        if nominal_power >= *CONSENSUS_MINER_MIN_POWER {
            self.num_miners_meeting_min_power += 1;
            self.total_network_power += claim.power;
        }

        Ok(())
    }

    pub(super) fn append_cron_event<BS: BlockStore>(
        &mut self,
        s: &BS,
        epoch: ChainEpoch,
        event: CronEvent,
    ) -> Result<(), String> {
        let mut mmap = Multimap::from_root(s, &self.cron_event_queue)?;
        mmap.add(epoch_key(epoch), event)?;
        self.cron_event_queue = mmap.root()?;
        Ok(())
    }

    pub(super) fn load_cron_events<BS: BlockStore>(
        &mut self,
        s: &BS,
        epoch: ChainEpoch,
    ) -> Result<Vec<CronEvent>, String> {
        let mut events = Vec::new();

        let mmap = Multimap::from_root(s, &self.cron_event_queue)?;
        mmap.for_each(&epoch_key(epoch), |_, v: &CronEvent| {
            match self.get_claim(s, &v.miner_addr) {
                Ok(Some(_)) => events.push(v.clone()),
                Err(e) => {
                    return Err(format!(
                        "failed to find claimed power for {} for cron event: {}",
                        v.miner_addr, e
                    ))
                }
                _ => (), // ignore events for defunct miners.
            }
            Ok(())
        })?;

        Ok(events)
    }

    pub(super) fn clear_cron_events<BS: BlockStore>(
        &mut self,
        s: &BS,
        epoch: ChainEpoch,
    ) -> Result<(), String> {
        let mut mmap = Multimap::from_root(s, &self.cron_event_queue)?;
        mmap.remove_all(&epoch_key(epoch))?;
        self.cron_event_queue = mmap.root()?;
        Ok(())
    }
}

fn epoch_key(e: ChainEpoch) -> BytesKey {
    // TODO switch logic to flip bits on negative value before encoding if ChainEpoch changed to i64
    // and add tests for edge cases once decided
    let ux = e << 1;
    let mut bz = unsigned_varint::encode::u64_buffer();
    unsigned_varint::encode::u64(ux, &mut bz);
    bz.to_vec().into()
}

impl Cbor for State {}
impl Serialize for State {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            BigUintDe(self.total_network_power.clone()),
            &self.miner_count,
            &self.escrow_table,
            &self.cron_event_queue,
            &self.last_epoch_tick,
            &self.post_detected_fault_miners,
            &self.claims,
            &self.num_miners_meeting_min_power,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for State {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (
            BigUintDe(total_network_power),
            miner_count,
            escrow_table,
            cron_event_queue,
            last_epoch_tick,
            post_detected_fault_miners,
            claims,
            num_miners_meeting_min_power,
        ) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            total_network_power,
            miner_count,
            escrow_table,
            cron_event_queue,
            last_epoch_tick,
            post_detected_fault_miners,
            claims,
            num_miners_meeting_min_power,
        })
    }
}

#[derive(Default, Debug)]
pub struct Claim {
    pub power: StoragePower,
    pub pledge: TokenAmount,
}

impl Serialize for Claim {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (BigUintSer(&self.power), BigUintSer(&self.pledge)).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Claim {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (BigUintDe(power), BigUintDe(pledge)) = Deserialize::deserialize(deserializer)?;
        Ok(Self { power, pledge })
    }
}

#[derive(Clone, Debug)]
pub struct CronEvent {
    pub miner_addr: Address,
    pub callback_payload: Serialized,
}

impl Cbor for CronEvent {}
impl Serialize for CronEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.miner_addr, &self.callback_payload).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CronEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (miner_addr, callback_payload) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            miner_addr,
            callback_payload,
        })
    }
}

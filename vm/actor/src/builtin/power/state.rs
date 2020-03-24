// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::CONSENSUS_MINER_MIN_POWER;
use crate::{BalanceTable, Set, StoragePower, HAMT_BIT_WIDTH};
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use ipld_blockstore::BlockStore;
use ipld_hamt::Hamt;
use num_bigint::{
    bigint_ser::{BigIntDe, BigIntSer},
    BigInt, Sign,
};
use num_traits::Zero;
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
    fn get_miner_balance<BS: BlockStore>(
        &self,
        store: &BS,
        miner: &Address,
    ) -> Result<TokenAmount, String> {
        let bt = BalanceTable::from_root(store, &self.escrow_table)?;
        bt.get(miner)
    }

    /// Sets miner balance at address using escrow table
    #[allow(dead_code)]
    fn set_miner_balance<BS: BlockStore>(
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
    #[allow(dead_code)]
    fn add_miner_balance<BS: BlockStore>(
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
    fn subtract_miner_balance<BS: BlockStore>(
        &mut self,
        store: &BS,
        miner: &Address,
        amount: &TokenAmount,
        balance_floor: &TokenAmount,
    ) -> Result<(), String> {
        let mut bt = BalanceTable::from_root(store, &self.escrow_table)?;
        bt.subtract_with_minimum(miner, amount, balance_floor)?;
        self.escrow_table = bt.root()?;
        Ok(())
    }
    /// Parameters may be negative to subtract
    pub fn add_to_claim<BS: BlockStore>(
        &mut self,
        store: &BS,
        miner: &Address,
        power: &StoragePower,
        pledge: &BigInt,
    ) -> Result<(), String> {
        let mut claim = self
            .get_claim(store, miner)?
            .ok_or(format!("no claim for actor {}", miner))?;

        let old_nominal_power = self.compute_nominal_power(store, miner, &claim.power)?;

        claim.power += power;
        claim.pledge += pledge;

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

        // Negative values check
        if claim.power.sign() == Sign::Minus {
            return Err(format!("negative claimed power: {}", claim.power));
        }
        if claim.pledge.sign() == Sign::Minus {
            return Err(format!("negative claimed pledge: {}", claim.pledge));
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
    fn get_claim<BS: BlockStore>(&self, store: &BS, a: &Address) -> Result<Option<Claim>, String> {
        let map: Hamt<String, _> = Hamt::load_with_bit_width(&self.claims, store, HAMT_BIT_WIDTH)?;

        Ok(map.get(&a.hash_key())?)
    }

    fn set_claim<BS: BlockStore>(
        &mut self,
        store: &BS,
        addr: &Address,
        claim: Claim,
    ) -> Result<(), String> {
        assert!(claim.power.sign() == Sign::Minus);
        assert!(claim.pledge.sign() == Sign::Minus);

        let mut map: Hamt<String, _> =
            Hamt::load_with_bit_width(&self.claims, store, HAMT_BIT_WIDTH)?;

        map.set(addr.hash_key(), claim)?;
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

    fn has_detected_fault<BS: BlockStore>(&self, store: &BS, a: &Address) -> Result<bool, String> {
        let faulty = Set::from_root(store, &self.post_detected_fault_miners)?;
        Ok(faulty.has(&a.hash_key())?)
    }
}

pub struct Claim {
    pub power: StoragePower,
    pub pledge: BigInt,
}

impl Serialize for Claim {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (BigIntSer(&self.power), BigIntSer(&self.pledge)).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Claim {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (BigIntDe(power), BigIntDe(pledge)) = Deserialize::deserialize(deserializer)?;
        Ok(Self { power, pledge })
    }
}

pub struct CronEvent {
    pub miner_addr: Address,
    // TODO revisit to make sure this should be this type
    pub callback_payload: Serialized,
}

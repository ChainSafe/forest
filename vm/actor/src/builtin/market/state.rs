// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{collateral_penalty_for_deal_activation_missed, DealProposal, DealState};
use crate::{BalanceTable, DealID, OptionalEpoch, SetMultimap};
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use encoding::Cbor;
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use num_traits::Zero;
use runtime::Runtime;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::{ActorError, ExitCode, TokenAmount};

/// Market actor state
#[derive(Default)]
pub struct State {
    /// Amt<DealID, DealProposal>
    pub proposals: Cid,
    /// Amt<DealID, DealState>
    pub states: Cid,
    /// Total amount held in escrow, indexed by actor address (including both locked and unlocked amounts).
    pub escrow_table: Cid,
    /// Amount locked, indexed by actor address.
    /// Note: the amounts in this table do not affect the overall amount in escrow:
    /// only the _portion_ of the total escrow amount that is locked.
    pub locked_table: Cid,
    /// Deal id state sequential incrementer
    pub next_id: DealID,
    /// Metadata cached for efficient iteration over deals.
    /// SetMultimap<Address>
    pub deal_ids_by_party: Cid,
}

impl State {
    pub fn new(empty_arr: Cid, empty_map: Cid, empty_mset: Cid) -> Self {
        Self {
            proposals: empty_arr.clone(),
            states: empty_arr,
            escrow_table: empty_map.clone(),
            locked_table: empty_map,
            next_id: 0,
            deal_ids_by_party: empty_mset,
        }
    }

    ////////////////////////////////////////////////////////////////////////////////
    // Balance table operations
    ////////////////////////////////////////////////////////////////////////////////

    pub fn add_escrow_balance<BS: BlockStore>(
        &mut self,
        store: &BS,
        a: &Address,
        amount: TokenAmount,
    ) -> Result<(), String> {
        let mut bt = BalanceTable::from_root(store, &self.escrow_table)?;
        bt.add_create(a, amount)?;

        self.escrow_table = bt.root()?;
        Ok(())
    }
    pub fn add_locked_balance<BS: BlockStore>(
        &mut self,
        store: &BS,
        a: &Address,
        amount: TokenAmount,
    ) -> Result<(), String> {
        let mut bt = BalanceTable::from_root(store, &self.locked_table)?;
        bt.add_create(a, amount)?;

        self.locked_table = bt.root()?;
        Ok(())
    }
    pub fn get_escrow_balance<BS: BlockStore>(
        &self,
        store: &BS,
        a: &Address,
    ) -> Result<TokenAmount, ActorError> {
        let bt = BalanceTable::from_root(store, &self.escrow_table).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("get escrow balance {}", e),
            )
        })?;
        bt.get(a).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("get escrow balance {}", e),
            )
        })
    }
    pub fn get_locked_balance<BS: BlockStore>(
        &self,
        store: &BS,
        a: &Address,
    ) -> Result<TokenAmount, ActorError> {
        let bt = BalanceTable::from_root(store, &self.locked_table).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("get locked balance {}", e),
            )
        })?;
        bt.get(a).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("get locked balance {}", e),
            )
        })
    }

    pub(super) fn maybe_lock_balance<BS: BlockStore>(
        &mut self,
        store: &BS,
        addr: &Address,
        amount: &TokenAmount,
    ) -> Result<(), ActorError> {
        let prev_locked = self.get_locked_balance(store, addr)?;
        let escrow_balance = self.get_locked_balance(store, addr)?;
        if &prev_locked + amount > escrow_balance {
            return Err(ActorError::new(
                ExitCode::ErrInsufficientFunds,
                format!(
                    "not enough balance to lock for addr {}: {} < {} + {}",
                    addr, escrow_balance, prev_locked, amount
                ),
            ));
        }

        let mut bt = BalanceTable::from_root(store, &self.locked_table)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
        bt.add(addr, amount).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("adding locked balance {}", e),
            )
        })?;
        self.locked_table = bt
            .root()
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
        Ok(())
    }

    pub(super) fn unlock_balance<BS: BlockStore>(
        &mut self,
        store: &BS,
        addr: &Address,
        amount: &TokenAmount,
    ) -> Result<(), ActorError> {
        let mut bt = BalanceTable::from_root(store, &self.locked_table)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

        bt.must_subtract(addr, amount).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("subtracting from locked balance: {}", e),
            )
        })?;
        self.locked_table = bt
            .root()
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
        Ok(())
    }

    pub(super) fn transfer_balance<BS: BlockStore>(
        &mut self,
        store: &BS,
        from_addr: &Address,
        to_addr: &Address,
        amount: &TokenAmount,
    ) -> Result<(), ActorError> {
        let mut et = BalanceTable::from_root(store, &self.escrow_table)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
        let mut lt = BalanceTable::from_root(store, &self.locked_table)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

        // Subtract from locked and escrow tables
        et.must_subtract(from_addr, &amount).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("subtract from escrow: {}", e),
            )
        })?;
        lt.must_subtract(from_addr, &amount).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("subtract from escrow: {}", e),
            )
        })?;

        // Add subtracted amount to the recipient
        et.add(to_addr, &amount).map_err(|e| {
            ActorError::new(ExitCode::ErrIllegalState, format!("add to escrow: {}", e))
        })?;

        // Update locked and escrow roots
        self.locked_table = lt
            .root()
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
        self.escrow_table = et
            .root()
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
        Ok(())
    }

    pub(super) fn slash_balance<BS: BlockStore>(
        &mut self,
        store: &BS,
        addr: &Address,
        amount: &TokenAmount,
    ) -> Result<(), ActorError> {
        let mut et = BalanceTable::from_root(store, &self.escrow_table)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
        let mut lt = BalanceTable::from_root(store, &self.locked_table)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

        // Subtract from locked and escrow tables
        et.must_subtract(addr, &amount).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("subtract from escrow: {}", e),
            )
        })?;
        lt.must_subtract(addr, &amount).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("subtract from escrow: {}", e),
            )
        })?;

        // Update locked and escrow roots
        self.locked_table = lt
            .root()
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
        self.escrow_table = et
            .root()
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
        Ok(())
    }

    ////////////////////////////////////////////////////////////////////////////////
    // Deal state operations
    ////////////////////////////////////////////////////////////////////////////////

    pub(super) fn update_pending_deal_states_for_party<BS, RT>(
        &mut self,
        rt: &RT,
        addr: &Address,
    ) -> Result<TokenAmount, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO check if rt curr_epoch can be 0
        let epoch = rt.curr_epoch() - 1;
        let dbp = SetMultimap::from_root(rt.store(), &self.deal_ids_by_party)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

        let mut extracted_ids = Vec::new();
        dbp.for_each(addr, |id| {
            extracted_ids.push(id);
            Ok(())
        })
        .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e))?;

        self.update_pending_deal_states(rt.store(), extracted_ids, epoch)
    }

    pub(super) fn update_pending_deal_states<BS>(
        &mut self,
        store: &BS,
        deal_ids: Vec<DealID>,
        epoch: ChainEpoch,
    ) -> Result<TokenAmount, ActorError>
    where
        BS: BlockStore,
    {
        let mut amount_slashed_total = TokenAmount::zero();

        for deal in deal_ids {
            amount_slashed_total += self.update_pending_deal_state(store, deal, epoch)?;
        }

        Ok(amount_slashed_total)
    }

    pub(super) fn update_pending_deal_state<BS>(
        &mut self,
        store: &BS,
        deal_id: DealID,
        epoch: ChainEpoch,
    ) -> Result<TokenAmount, ActorError>
    where
        BS: BlockStore,
    {
        let deal = self.must_get_deal(store, deal_id)?;
        let mut state = self.must_get_deal_state(store, deal_id)?;

        let ever_updated = state.last_updated_epoch.is_some();
        let ever_slashed = state.slash_epoch.is_some();

        if ever_updated && state.last_updated_epoch.unwrap() > epoch {
            return Err(ActorError::new(
                ExitCode::ErrIllegalState,
                "Deal was updated in the future".to_owned(),
            ));
        }

        if state.sector_start_epoch.is_none() {
            if epoch > deal.start_epoch {
                return self.process_deal_init_timed_out(store, deal_id, deal, state);
            }
            return Ok(TokenAmount::zero());
        }

        assert!(
            deal.start_epoch <= epoch,
            "Deal start cannot exceed current epoch"
        );

        let deal_end = if ever_slashed {
            assert!(
                state.slash_epoch.unwrap() <= deal.end_epoch,
                "Epoch slashed must be less or equal to the end epoch"
            );
            state.slash_epoch.unwrap()
        } else {
            deal.end_epoch
        };

        let elapsed_start = if ever_updated && state.last_updated_epoch.unwrap() > deal.start_epoch
        {
            state.last_updated_epoch.unwrap()
        } else {
            deal.start_epoch
        };

        let elapsed_end = if epoch < deal_end { epoch } else { deal_end };

        let num_epochs_elapsed = elapsed_end - elapsed_start;

        self.transfer_balance(
            store,
            &deal.client,
            &deal.provider,
            &(deal.storage_price_per_epoch.clone() * num_epochs_elapsed),
        )?;

        if ever_slashed {
            let payment_remaining = deal_get_payment_remaining(&deal, state.slash_epoch.unwrap());
            self.unlock_balance(
                store,
                &deal.client,
                &(payment_remaining + &deal.client_collateral),
            )?;

            let slashed = deal.provider_collateral.clone();
            self.slash_balance(store, &deal.provider, &slashed)?;

            self.delete_deal(store, deal_id, deal)?;
            return Ok(slashed);
        }

        if epoch >= deal.end_epoch {
            self.process_deal_expired(store, deal_id, deal, state)?;
            return Ok(TokenAmount::zero());
        }

        state.last_updated_epoch = OptionalEpoch(Some(epoch));

        // Update states array
        let mut states = Amt::<DealState, _>::load(&self.states, store)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
        states
            .set(deal_id, state)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
        self.states = states
            .flush()
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
        Ok(TokenAmount::zero())
    }

    pub(super) fn delete_deal<BS>(
        &mut self,
        store: &BS,
        deal_id: DealID,
        deal: DealProposal,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
    {
        // let deal = self.must_get_deal(store, deal_id)?;

        let mut proposals = Amt::<DealState, _>::load(&self.proposals, store)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
        proposals
            .delete(deal_id)
            .map_err(|e| ActorError::new(ExitCode::ErrPlaceholder, e.into()))?;

        let mut dbp = SetMultimap::from_root(store, &self.deal_ids_by_party)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
        dbp.remove(&deal.client, deal_id)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e))?;

        // Update roots of update
        self.proposals = proposals
            .flush()
            .map_err(|e| ActorError::new(ExitCode::ErrPlaceholder, e.into()))?;
        self.deal_ids_by_party = dbp
            .root()
            .map_err(|e| ActorError::new(ExitCode::ErrPlaceholder, e.into()))?;
        Ok(())
    }

    pub(super) fn process_deal_init_timed_out<BS>(
        &mut self,
        store: &BS,
        deal_id: DealID,
        deal: DealProposal,
        state: DealState,
    ) -> Result<TokenAmount, ActorError>
    where
        BS: BlockStore,
    {
        assert!(
            state.sector_start_epoch.is_none(),
            "Sector start epoch must be undefined"
        );
        self.unlock_balance(store, &deal.client, &deal.client_balance_requirement())?;

        let amount_slashed =
            collateral_penalty_for_deal_activation_missed(deal.provider_collateral.clone());
        let amount_remainging = deal.provider_balance_requirement() - &amount_slashed;

        self.slash_balance(store, &deal.provider, &amount_slashed)?;
        self.unlock_balance(store, &deal.provider, &amount_remainging)?;
        self.delete_deal(store, deal_id, deal)?;
        Ok(amount_slashed)
    }

    pub(super) fn process_deal_expired<BS>(
        &mut self,
        store: &BS,
        deal_id: DealID,
        deal: DealProposal,
        state: DealState,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
    {
        assert!(
            state.sector_start_epoch.is_some(),
            "Sector start epoch must be initialized at this point"
        );

        self.unlock_balance(store, &deal.provider, &deal.provider_collateral)?;
        self.unlock_balance(store, &deal.client, &deal.client_collateral)?;

        self.delete_deal(store, deal_id, deal)
    }
    #[allow(dead_code)]
    pub(super) fn generate_storage_deal_id(&mut self) -> DealID {
        let ret = self.next_id;
        self.next_id += 1;
        ret
    }

    ////////////////////////////////////////////////////////////////////////////////
    // Method utility functions
    ////////////////////////////////////////////////////////////////////////////////

    pub(super) fn must_get_deal<BS: BlockStore>(
        &self,
        store: &BS,
        deal_id: DealID,
    ) -> Result<DealProposal, ActorError> {
        let proposals = Amt::load(&self.proposals, store)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
        Ok(proposals
            .get(deal_id)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("get proposal for id {}: {}", deal_id, e),
                )
            })?
            .ok_or_else(|| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("proposal not found for id {}", deal_id),
                )
            })?)
    }

    pub(super) fn must_get_deal_state<BS: BlockStore>(
        &self,
        store: &BS,
        deal_id: DealID,
    ) -> Result<DealState, ActorError> {
        let states = Amt::load(&self.states, store)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
        Ok(states
            .get(deal_id)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("get deal state for id {}: {}", deal_id, e),
                )
            })?
            .ok_or_else(|| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("deal state not found for id {}", deal_id),
                )
            })?)
    }

    pub(super) fn lock_balance_or_abort<BS: BlockStore>(
        &mut self,
        store: &BS,
        addr: &Address,
        amount: &TokenAmount,
    ) -> Result<(), ActorError> {
        if amount < &TokenAmount::zero() {
            return Err(ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!("negative amount {}", amount),
            ));
        }

        self.maybe_lock_balance(store, addr, amount)
    }
}

fn deal_get_payment_remaining(deal: &DealProposal, epoch: ChainEpoch) -> TokenAmount {
    assert!(
        epoch <= deal.end_epoch,
        "Current epoch must be before the end epoch of the deal"
    );

    let duration_remaining = deal.end_epoch - (epoch - 1);

    deal.storage_price_per_epoch.clone() * duration_remaining
}

impl Cbor for State {}

impl Serialize for State {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.proposals,
            &self.states,
            &self.escrow_table,
            &self.locked_table,
            &self.next_id,
            &self.deal_ids_by_party,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for State {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (proposals, states, escrow_table, locked_table, next_id, deal_ids_by_party) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            proposals,
            states,
            escrow_table,
            locked_table,
            next_id,
            deal_ids_by_party,
        })
    }
}

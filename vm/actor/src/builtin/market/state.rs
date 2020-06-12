// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{
    collateral_penalty_for_deal_activation_missed, DealProposal, DealState, DEAL_UPDATED_INTERVAL,
};
use crate::{BalanceTable, DealID, OptionalEpoch};
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use encoding::tuple::*;
use encoding::Cbor;
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use num_traits::Zero;
use vm::{ActorError, ExitCode, TokenAmount};

/// Market actor state
#[derive(Default, Serialize_tuple, Deserialize_tuple)]
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
    pub deal_ops_by_epoch: Cid,
    pub last_cron: ChainEpoch,
}

impl State {
    pub fn new(empty_arr: Cid, empty_map: Cid, empty_mset: Cid) -> Self {
        Self {
            proposals: empty_arr.clone(),
            states: empty_arr,
            escrow_table: empty_map.clone(),
            locked_table: empty_map,
            next_id: 0,
            deal_ops_by_epoch: empty_mset,
            last_cron: ChainEpoch::default(),
        }
    }

    ////////////////////////////////////////////////////////////////////////////////
    // Deal state operations
    ////////////////////////////////////////////////////////////////////////////////
    #[allow(clippy::too_many_arguments)]
    pub(super) fn update_pending_deal_state<BS>(
        &mut self,
        store: &BS,
        state: DealState,
        deal: DealProposal,
        deal_id: DealID,
        et: &mut BalanceTable<BS>,
        lt: &mut BalanceTable<BS>,
        epoch: ChainEpoch,
    ) -> Result<(TokenAmount, OptionalEpoch), ActorError>
    where
        BS: BlockStore,
    {
        let ever_updated = state.last_updated_epoch.is_some();
        let ever_slashed = state.slash_epoch.is_some();

        // if the deal was ever updated, make sure it didn't happen in the future
        assert!(!ever_updated || state.last_updated_epoch.unwrap() <= epoch);

        // This would be the case that the first callback somehow triggers before it is scheduled to
        // This is expected not to be able to happen
        if deal.start_epoch > epoch {
            return Ok((TokenAmount::zero(), OptionalEpoch(None)));
        }

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

        let elapsed_end = std::cmp::min(epoch, deal_end);

        let num_epochs_elapsed = elapsed_end - elapsed_start;

        self.transfer_balance(
            store,
            &deal.client,
            &deal.provider,
            &(deal.storage_price_per_epoch.clone() * num_epochs_elapsed),
        )?;

        if ever_slashed {
            // unlock client collateral and locked storage fee
            let payment_remaining = deal_get_payment_remaining(&deal, state.slash_epoch.unwrap());
            // specs actors are not handling this err
            self.unlock_balance(
                lt,
                &deal.client,
                &(payment_remaining + &deal.client_collateral),
            )
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalArgument, e))?;

            // slash provider collateral
            let slashed = deal.provider_collateral.clone();
            self.slash_balance(et, lt, &deal.provider, &slashed)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("slashing balance: {}", e),
                    )
                })?;

            self.delete_deal(store, deal_id)?;
            return Ok((slashed, OptionalEpoch(None)));
        }

        if epoch >= deal.end_epoch {
            self.process_deal_expired(store, deal_id, &deal, state, lt)?;
            return Ok((TokenAmount::zero(), OptionalEpoch(None)));
        }

        let next: ChainEpoch = std::cmp::min(epoch + DEAL_UPDATED_INTERVAL, deal.end_epoch);

        Ok((TokenAmount::zero(), OptionalEpoch(Some(next))))
    }
    fn mutate_deal_proposals<BS, F>(&mut self, store: &BS, f: F) -> Result<(), ActorError>
    where
        F: FnOnce(&mut Amt<Cid, BS>) -> Result<(), ActorError>,
        BS: BlockStore,
    {
        let mut prop = Amt::load(&self.proposals, store)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

        f(&mut prop)?;

        let r_cid = prop.flush().map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("flushing deal proposals set failed: {}", e),
            )
        })?;

        self.proposals = r_cid;
        Ok(())
    }

    fn delete_deal<BS>(&mut self, store: &BS, deal_id: DealID) -> Result<(), ActorError>
    where
        BS: BlockStore,
    {
        self.mutate_deal_proposals(store, |props: &mut Amt<Cid, BS>| {
            props.delete(deal_id).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrPlaceholder,
                    format!("failed to delete deal: {}", e),
                )
            })?;
            Ok(())
        })?;

        Ok(())
    }

    /// Deal start deadline elapsed without appearing in a proven sector.
    /// Delete deal, slash a portion of provider's collateral, and unlock remaining collaterals
    /// for both provider and client.
    pub(super) fn process_deal_init_timed_out<BS>(
        &mut self,
        store: &BS,
        lt: &mut BalanceTable<BS>,
        et: &mut BalanceTable<BS>,
        deal_id: DealID,
        deal: &DealProposal,
        state: DealState,
    ) -> Result<TokenAmount, ActorError>
    where
        BS: BlockStore,
    {
        assert!(
            state.sector_start_epoch.is_none(),
            "Sector start epoch must be undefined"
        );

        // specs actors not handling this err
        self.unlock_balance(lt, &deal.client, &deal.client_balance_requirement())
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalArgument, e))?;

        let amount_slashed =
            collateral_penalty_for_deal_activation_missed(deal.provider_collateral.clone());
        let amount_remaining = deal.provider_balance_requirement() - &amount_slashed;

        self.slash_balance(et, lt, &deal.provider, &amount_slashed)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to slash balance: {}", e),
                )
            })?;

        // specs actors not handling this err
        self.unlock_balance(lt, &deal.provider, &amount_remaining)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalArgument, e))?;

        self.delete_deal(store, deal_id)?;
        Ok(amount_slashed)
    }

    fn process_deal_expired<BS>(
        &mut self,
        store: &BS,
        deal_id: DealID,
        deal: &DealProposal,
        state: DealState,
        lt: &mut BalanceTable<BS>,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
    {
        assert!(
            state.sector_start_epoch.is_some(),
            "Sector start epoch must be initialized at this point"
        );

        self.unlock_balance(lt, &deal.provider, &deal.provider_collateral)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalArgument, e))?;

        self.unlock_balance(lt, &deal.client, &deal.client_collateral)
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalArgument, e))?;

        self.delete_deal(store, deal_id)
    }

    pub(super) fn generate_storage_deal_id(&mut self) -> DealID {
        let ret = self.next_id;
        self.next_id += 1;
        ret
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
        mutate_balance_table(
            store,
            &mut self.escrow_table,
            |et: &mut BalanceTable<BS>| {
                et.add_create(a, amount)?;
                Ok(())
            },
        )?;

        Ok(())
    }
    pub fn add_locked_balance<BS: BlockStore>(
        &mut self,
        store: &BS,
        a: &Address,
        amount: TokenAmount,
    ) -> Result<(), String> {
        mutate_balance_table(
            store,
            &mut self.locked_table,
            |lt: &mut BalanceTable<BS>| {
                lt.add_create(a, amount)?;
                Ok(())
            },
        )?;

        Ok(())
    }
    fn get_escrow_balance<BS: BlockStore>(
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

    fn maybe_lock_balance<BS: BlockStore>(
        &mut self,
        store: &BS,
        addr: &Address,
        amount: &TokenAmount,
    ) -> Result<(), ActorError> {
        let prev_locked = self.get_locked_balance(store, addr)?;
        let escrow_balance = self.get_escrow_balance(store, addr)?;
        if &prev_locked + amount > escrow_balance {
            return Err(ActorError::new(
                ExitCode::ErrInsufficientFunds,
                format!(
                    "not enough balance to lock for addr {}: {} <  {}",
                    addr,
                    prev_locked + amount,
                    escrow_balance
                ),
            ));
        }

        mutate_balance_table(
            store,
            &mut self.locked_table,
            |lt: &mut BalanceTable<BS>| {
                lt.add(addr, amount)?;
                Ok(())
            },
        )
        .map_err(|e| ActorError::new(ExitCode::ErrPlaceholder, e))?;

        Ok(())
    }
    fn unlock_balance<BS: BlockStore>(
        &mut self,
        lt: &mut BalanceTable<BS>,
        addr: &Address,
        amount: &TokenAmount,
    ) -> Result<(), String> {
        lt.must_subtract(addr, amount)?;

        Ok(())
    }
    /// move funds from locked in client to available in provider
    fn transfer_balance<BS: BlockStore>(
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
                format!("subtract from locked: {}", e),
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

    fn slash_balance<BS: BlockStore>(
        &mut self,
        et: &mut BalanceTable<BS>,
        lt: &mut BalanceTable<BS>,
        addr: &Address,
        amount: &TokenAmount,
    ) -> Result<(), String> {
        // Subtract from locked and escrow tables
        et.must_subtract(addr, &amount)?;
        lt.must_subtract(addr, &amount)?;

        Ok(())
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

fn mutate_balance_table<BS, F>(store: &BS, c: &mut Cid, f: F) -> Result<(), String>
where
    F: FnOnce(&mut BalanceTable<BS>) -> Result<(), String>,
    BS: BlockStore,
{
    let mut t = BalanceTable::from_root(store, &c)?;

    f(&mut t)?;

    *c = t.root()?;
    Ok(())
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

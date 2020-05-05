// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod deal;
mod policy;
mod state;
mod types;

pub use self::deal::*;
use self::policy::*;
pub use self::state::State;
pub use self::types::*;
use crate::{
    make_map, request_miner_control_addrs, BalanceTable, DealID, DealWeight, OptionalEpoch,
    SetMultimap, BURNT_FUNDS_ACTOR_ADDR, CALLER_TYPES_SIGNABLE, MINER_ACTOR_CODE_ID,
    SYSTEM_ACTOR_ADDR,
};
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use encoding::to_vec;
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use message::Message;
use num_bigint::bigint_ser::BigIntSer;
use num_bigint::{BigInt, BigUint};
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Zero};
use runtime::{ActorCode, Runtime};
use vm::{
    ActorError, ExitCode, MethodNum, PieceInfo, Serialized, TokenAmount, METHOD_CONSTRUCTOR,
    METHOD_SEND,
};

/// Market actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    AddBalance = 2,
    WithdrawBalance = 3,
    HandleExpiredDeals = 4,
    PublishStorageDeals = 5,
    VerifyDealsOnSectorProveCommit = 6,
    OnMinerSectorsTerminate = 7,
    ComputeDataCommitment = 8,
}
/// Market Actor
pub struct Actor;
impl Actor {
    pub fn constructor<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*SYSTEM_ACTOR_ADDR))?;

        let empty_root = Amt::<Cid, BS>::new(rt.store()).flush().map_err(|e| {
            rt.abort(
                ExitCode::ErrIllegalState,
                format!("Failed to create market state: {}", e),
            )
        })?;

        let empty_map = make_map(rt.store()).flush().map_err(|err| {
            rt.abort(
                ExitCode::ErrIllegalState,
                format!("Failed to create market state: {}", err),
            )
        })?;

        let empty_m_set = SetMultimap::new(rt.store()).root().map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("Failed to create market state: {}", e),
            )
        })?;

        let st = State::new(empty_root, empty_map, empty_m_set);
        rt.create(&st)?;
        Ok(())
    }

    /// Deposits the received value into the balance held in escrow.
    fn add_balance<BS, RT>(rt: &mut RT, provider_or_client: Address) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let (nominal, _) = escrow_address(rt, &provider_or_client)?;

        let msg_value = rt.message().value().clone();
        rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
            st.add_escrow_balance(rt.store(), &nominal, msg_value)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("adding to escrow table: {}", e),
                    )
                })?;

            // ensure there is an entry in the locked table
            st.add_locked_balance(rt.store(), &nominal, TokenAmount::zero())
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        format!("adding to locked table: {}", e),
                    )
                })?;
            Ok(())
        })??;

        Ok(())
    }

    /// Attempt to withdraw the specified amount from the balance held in escrow.
    /// If less than the specified amount is available, yields the entire available balance.
    fn withdraw_balance<BS, RT>(
        rt: &mut RT,
        params: WithdrawBalanceParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let (nominal, recipient) = escrow_address(rt, &params.provider_or_client)?;

        let mut amount_slashed_total = TokenAmount::zero();
        let amount_extracted =
            rt.transaction::<_, Result<TokenAmount, ActorError>, _>(|st: &mut State, rt| {
                // Before any operations that check the balance tables for funds, execute all deferred
                // deal state updates.
                amount_slashed_total += st.update_pending_deal_states_for_party(rt, &nominal)?;

                let min_balance = st.get_locked_balance(rt.store(), &nominal)?;

                let mut et = BalanceTable::from_root(rt.store(), &st.escrow_table)
                    .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
                let ex = et
                    .subtract_with_minimum(&nominal, &params.amount, &min_balance)
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("Subtract form escrow table: {}", e),
                        )
                    })?;

                st.escrow_table = et
                    .root()
                    .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

                Ok(ex)
            })??;

        if amount_slashed_total > BigUint::zero() {
            rt.send(
                &*BURNT_FUNDS_ACTOR_ADDR,
                METHOD_SEND,
                &Serialized::default(),
                &amount_slashed_total,
            )?;
        }
        rt.send(
            &recipient,
            METHOD_SEND,
            &Serialized::default(),
            &amount_extracted,
        )?;
        Ok(())
    }

    fn handle_expired_deals<BS, RT>(
        rt: &mut RT,
        params: HandleExpiredDealsParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;

        let slashed = rt.transaction(|st: &mut State, rt| {
            st.update_pending_deal_states(rt.store(), params.deal_ids, rt.curr_epoch())
        })??;

        // TODO: award some small portion of slashed to caller as incentive

        rt.send(
            &*BURNT_FUNDS_ACTOR_ADDR,
            METHOD_SEND,
            &Serialized::default(),
            &slashed,
        )?;
        Ok(())
    }

    /// Publish a new set of storage deals (not yet included in a sector).
    fn publish_storage_deals<BS, RT>(
        rt: &mut RT,
        params: PublishStorageDealsParams,
    ) -> Result<PublishStorageDealsReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let mut amount_slashed_total = TokenAmount::zero();

        // Deal message must have a From field identical to the provider of all the deals.
        // This allows us to retain and verify only the client's signature in each deal proposal itself.
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;
        if params.deals.is_empty() {
            return Err(ActorError::new(
                ExitCode::ErrIllegalArgument,
                "Empty deals parameter.".to_owned(),
            ));
        }

        // All deals should have the same provider so get worker once
        let provider_raw = &params.deals[0].proposal.provider;
        let provider = rt.resolve_address(&provider_raw)?;

        let (_, worker) = request_miner_control_addrs(rt, &provider)?;
        if &worker != rt.message().from() {
            return Err(ActorError::new(
                ExitCode::ErrForbidden,
                format!("Caller is not provider {}", worker),
            ));
        }

        // All deals should have the same provider so get worker once
        let provider_raw = params.deals[0].proposal.provider;
        let provider = rt.resolve_address(&provider_raw)?;

        let mut new_deal_ids: Vec<DealID> = Vec::new();
        rt.transaction(|st: &mut State, rt| {
            let mut prop = Amt::load(&st.proposals, rt.store())
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
            let mut dbp = SetMultimap::from_root(rt.store(), &st.deal_ids_by_party)
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

            for mut deal in params.deals {
                validate_deal(rt, &deal)?;

                if deal.proposal.provider != provider && deal.proposal.provider != provider_raw {
                    return Err(ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        "Cannot publish deals from different providers at the same time."
                            .to_owned(),
                    ));
                }

                let client = rt.resolve_address(&deal.proposal.client)?;
                // Normalise provider and client addresses in the proposal stored on chain (after signature verification).
                deal.proposal.provider = provider;
                deal.proposal.client = client;

                // Before any operations that check the balance tables for funds, execute all deferred
                // deal state updates.
                //
                // Note: as an optimization, implementations may cache efficient data structures indicating
                // which of the following set of updates are redundant and can be skipped.
                amount_slashed_total += st.update_pending_deal_states_for_party(rt, &client)?;
                amount_slashed_total += st.update_pending_deal_states_for_party(rt, &provider)?;

                st.lock_balance_or_abort(
                    rt.store(),
                    &client,
                    &deal.proposal.client_balance_requirement(),
                )?;
                st.lock_balance_or_abort(
                    rt.store(),
                    &provider,
                    deal.proposal.provider_balance_requirement(),
                )?;

                let id = st.generate_storage_deal_id();

                prop.set(id, deal.proposal)
                    .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
                dbp.put(&client, id)
                    .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e))?;
                dbp.put(&provider, id)
                    .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e))?;

                new_deal_ids.push(id);
            }
            st.proposals = prop
                .flush()
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
            st.deal_ids_by_party = dbp
                .root()
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
            Ok(())
        })??;

        rt.send(
            &*BURNT_FUNDS_ACTOR_ADDR,
            METHOD_SEND,
            &Serialized::default(),
            &amount_slashed_total,
        )?;

        Ok(PublishStorageDealsReturn { ids: new_deal_ids })
    }

    /// Verify that a given set of storage deals is valid for a sector currently being ProveCommitted,
    /// update the market's internal state accordingly, and return DealWeight of the set of storage deals given.
    /// Note: in the case of a capacity-commitment sector (one with zero deals), this function should succeed vacuously.
    /// The weight is defined as the sum, over all deals in the set, of the product of its size
    /// with its duration. This quantity may be an input into the functions specifying block reward,
    /// sector power, collateral, and/or other parameters.    
    fn verify_deals_on_sector_prove_commit<BS, RT>(
        rt: &mut RT,
        params: VerifyDealsOnSectorProveCommitParams,
    ) -> Result<DealWeight, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        let miner_addr = *rt.message().from();
        let mut total_deal_space_time = BigInt::zero();
        let mut deal_weight = BigInt::zero();

        rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
            // if there are no dealIDs, it is a CommittedCapacity sector
            // and the totalDealSpaceTime should be zero
            let mut states = Amt::load(&st.states, rt.store())
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
            let proposals = Amt::load(&st.proposals, rt.store())
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

            for id in &params.deal_ids {
                let mut deal: DealState = states
                    .get(*id)
                    .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?
                    .ok_or_else(|| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            "Failed to retrieve the DealState".to_owned(),
                        )
                    })?;
                let proposal: DealProposal = proposals
                    .get(*id)
                    .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?
                    .ok_or_else(|| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            "Failed to retrieve the DealProposal".to_owned(),
                        )
                    })?;

                validate_deal_can_activate(
                    rt.curr_epoch(),
                    &miner_addr,
                    params.sector_expiry,
                    &deal,
                    &proposal,
                )?;

                deal.sector_start_epoch = OptionalEpoch(Some(rt.curr_epoch()));
                states
                    .set(*id, deal)
                    .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

                // compute deal weight
                let deal_space_time = proposal.duration() * proposal.piece_size.0;
                total_deal_space_time += deal_space_time;
            }
            st.states = states
                .flush()
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

            let epoch_value = params
                .sector_expiry
                .checked_sub(rt.curr_epoch())
                .unwrap_or(1u64);

            let sector_space_time = BigInt::from(params.sector_size as u64) * epoch_value;
            deal_weight = total_deal_space_time / sector_space_time;

            Ok(())
        })??;

        Ok(deal_weight)
    }

    /// Terminate a set of deals in response to their containing sector being terminated.
    /// Slash provider collateral, refund client collateral, and refund partial unpaid escrow
    /// amount to client.    
    fn on_miners_sector_terminate<BS, RT>(
        rt: &mut RT,
        params: OnMinerSectorsTerminateParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        let miner_addr = *rt.message().from();

        rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
            let prop = Amt::load(&st.proposals, rt.store())
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
            let mut states = Amt::load(&st.states, rt.store())
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

            for id in params.deal_ids {
                let deal: DealProposal = prop
                    .get(id)
                    .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?
                    .ok_or_else(|| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            "Failed to retrieve DealProposal".to_owned(),
                        )
                    })?;
                assert_eq!(deal.provider, miner_addr);

                let mut state: DealState = states
                    .get(id)
                    .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?
                    .ok_or_else(|| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            "Failed to retrieve DealState".to_owned(),
                        )
                    })?;

                // Note: we do not perform the balance transfers here, but rather simply record the flag
                // to indicate that processDealSlashed should be called when the deferred state computation
                // is performed. // TODO: Do that here

                state.slash_epoch = OptionalEpoch(Some(rt.curr_epoch()));
                states.set(id, state).map_err(|e| {
                    ActorError::new(ExitCode::ErrIllegalState, format!("Set deal error: {}", e))
                })?;
            }

            st.states = states
                .flush()
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
            Ok(())
        })??;
        Ok(())
    }

    fn compute_data_commitment<BS, RT>(
        rt: &mut RT,
        params: ComputeDataCommitmentParams,
    ) -> Result<Cid, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;

        let mut pieces: Vec<PieceInfo> = Vec::new();
        rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
            for id in &params.deal_ids {
                let deal = st.must_get_deal(rt.store(), *id)?;
                pieces.push(PieceInfo {
                    size: deal.piece_size,
                    cid: deal.piece_cid,
                });
            }
            Ok(())
        })??;

        let commd = rt
            .syscalls()
            .compute_unsealed_sector_cid(params.sector_type, &pieces)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::SysErrorIllegalArgument,
                    format!("failed to compute unsealed sector CID: {}", e),
                )
            })?;

        Ok(commd)
    }
}
////////////////////////////////////////////////////////////////////////////////
// Checks
////////////////////////////////////////////////////////////////////////////////
fn validate_deal_can_activate(
    curr_epoch: ChainEpoch,
    miner_addr: &Address,
    sector_exp: ChainEpoch,
    deal: &DealState,
    proposal: &DealProposal,
) -> Result<(), ActorError> {
    if &proposal.provider != miner_addr {
        return Err(ActorError::new(
            ExitCode::ErrIllegalArgument,
            "Deal has incorrect miner as its provider.".to_owned(),
        ));
    };

    if deal.sector_start_epoch.is_some() {
        return Err(ActorError::new(
            ExitCode::ErrIllegalArgument,
            "Deal has already appeared in proven sector.".to_owned(),
        ));
    }

    if curr_epoch > proposal.start_epoch {
        return Err(ActorError::new(
            ExitCode::ErrIllegalArgument,
            "Deal start epoch has already elapsed.".to_owned(),
        ));
    };

    if proposal.end_epoch > sector_exp {
        return Err(ActorError::new(
            ExitCode::ErrIllegalArgument,
            "Deal would outlive its containing sector.".to_owned(),
        ));
    };

    Ok(())
}

fn validate_deal<BS, RT>(rt: &RT, deal: &ClientDealProposal) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    deal_proposal_is_internally_valid(rt, deal)?;

    if rt.curr_epoch() > deal.proposal.start_epoch {
        return Err(ActorError::new(
            ExitCode::ErrIllegalArgument,
            "Deal start epoch has already elapsed.".to_owned(),
        ));
    };

    let (min_dur, max_dur) = deal_duration_bounds(deal.proposal.piece_size);
    if deal.proposal.duration() < min_dur || deal.proposal.duration() > max_dur {
        return Err(ActorError::new(
            ExitCode::ErrIllegalArgument,
            "Deal duration out of bounds.".to_owned(),
        ));
    };

    let (min_price, max_price) =
        deal_price_per_epoch_bounds(deal.proposal.piece_size, deal.proposal.duration());
    if deal.proposal.storage_price_per_epoch < min_price
        || deal.proposal.storage_price_per_epoch > max_price
    {
        return Err(ActorError::new(
            ExitCode::ErrIllegalArgument,
            "Storage price out of bounds.".to_owned(),
        ));
    };

    let (min_provider_collateral, max_provider_collateral) =
        deal_provider_collateral_bounds(deal.proposal.piece_size, deal.proposal.duration());
    if deal.proposal.provider_collateral < min_provider_collateral
        || deal.proposal.provider_collateral > max_provider_collateral
    {
        return Err(ActorError::new(
            ExitCode::ErrIllegalArgument,
            "Provider collateral out of bounds.".to_owned(),
        ));
    };

    let (min_client_collateral, max_client_collateral) =
        deal_client_collateral_bounds(deal.proposal.piece_size, deal.proposal.duration());
    if deal.proposal.provider_collateral < min_client_collateral
        || deal.proposal.provider_collateral > max_client_collateral
    {
        return Err(ActorError::new(
            ExitCode::ErrIllegalArgument,
            "Client collateral out of bounds.".to_owned(),
        ));
    };

    Ok(())
}

fn deal_proposal_is_internally_valid<BS, RT>(
    rt: &RT,
    proposal: &ClientDealProposal,
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if proposal.proposal.end_epoch <= proposal.proposal.start_epoch {
        return Err(ActorError::new(
            ExitCode::ErrIllegalArgument,
            "proposal end epoch before start epoch".to_owned(),
        ));
    }
    // Generate unsigned bytes
    let sv_bz = to_vec(&proposal.proposal).map_err(|_| {
        rt.abort(
            ExitCode::ErrIllegalArgument,
            "failed to serialize DealProposal",
        )
    })?;

    rt.syscalls()
        .verify_signature(
            &proposal.client_signature,
            &proposal.proposal.client,
            &sv_bz,
        )
        .map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!("signature proposal invalid: {}", e),
            )
        })?;

    Ok(())
}

// Resolves a provider or client address to the canonical form against which a balance should be held, and
// the designated recipient address of withdrawals (which is the same, for simple account parties).
fn escrow_address<BS, RT>(rt: &RT, addr: &Address) -> Result<(Address, Address), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    // Resolve the provided address to the canonical form against which the balance is held.
    let nominal = rt.resolve_address(addr).map_err(|e| {
        ActorError::new(
            ExitCode::ErrIllegalArgument,
            format!("Failed to resolve address provided: {}", e),
        )
    })?;

    let code_id = rt.get_actor_code_cid(&nominal).map_err(|e| {
        ActorError::new(
            ExitCode::ErrIllegalArgument,
            format!("Failed to retrieve actor code cid: {}", e),
        )
    })?;

    if code_id != *MINER_ACTOR_CODE_ID {
        // Ordinary account-style actor entry; funds recipient is just the entry address itself.
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;
        return Ok((nominal, nominal));
    }

    // Storage miner actor entry; implied funds recipient is the associated owner address.
    let (owner_addr, worker_addr) = request_miner_control_addrs(rt, &nominal)?;
    rt.validate_immediate_caller_is([owner_addr, worker_addr].iter())?;
    Ok((nominal, owner_addr))
}

impl ActorCode for Actor {
    fn invoke_method<BS, RT>(
        &self,
        rt: &mut RT,
        method: MethodNum,
        params: &Serialized,
    ) -> Result<Serialized, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match FromPrimitive::from_u64(method) {
            Some(Method::Constructor) => {
                Self::constructor(rt)?;
                Ok(Serialized::default())
            }
            Some(Method::AddBalance) => {
                Self::add_balance(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::WithdrawBalance) => {
                Self::withdraw_balance(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::HandleExpiredDeals) => {
                Self::handle_expired_deals(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::PublishStorageDeals) => {
                let res = Self::publish_storage_deals(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::VerifyDealsOnSectorProveCommit) => {
                let res = Self::verify_deals_on_sector_prove_commit(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(BigIntSer(&res))?)
            }
            Some(Method::OnMinerSectorsTerminate) => {
                Self::on_miners_sector_terminate(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::ComputeDataCommitment) => {
                let res = Self::compute_data_commitment(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(res)?)
            }
            _ => Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method")),
        }
    }
}

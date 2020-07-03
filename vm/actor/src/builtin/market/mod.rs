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
    make_map, request_miner_control_addrs,
    verifreg::{BytesParams, Method as VerifregMethod},
    BalanceTable, DealID, SetMultimap, BURNT_FUNDS_ACTOR_ADDR, CALLER_TYPES_SIGNABLE,
    CRON_ACTOR_ADDR, MINER_ACTOR_CODE_ID, SYSTEM_ACTOR_ADDR, VERIFIED_REGISTRY_ACTOR_ADDR,
};
use address::Address;
use cid::Cid;
use clock::{ChainEpoch, EPOCH_UNDEFINED};
use encoding::to_vec;
use fil_types::PieceInfo;
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use message::Message;
use num_bigint::BigUint;
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Zero};
use runtime::{ActorCode, Runtime};
use vm::{
    ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR, METHOD_SEND,
};

/// Market actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    AddBalance = 2,
    WithdrawBalance = 3,
    PublishStorageDeals = 4,
    VerifyDealsOnSectorProveCommit = 5,
    OnMinerSectorsTerminate = 6,
    ComputeDataCommitment = 7,
    CronTick = 8,
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

        let amount_slashed_total = TokenAmount::zero();
        let amount_extracted =
            rt.transaction::<_, Result<TokenAmount, ActorError>, _>(|st: &mut State, rt| {
                // The withdrawable amount might be slightly less than nominal
                // depending on whether or not all relevant entries have been processed
                // by cron

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

        // TODO this will never be hit
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

    /// Publish a new set of storage deals (not yet included in a sector).
    fn publish_storage_deals<BS, RT>(
        rt: &mut RT,
        params: PublishStorageDealsParams,
    ) -> Result<PublishStorageDealsReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
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

        for deal in &params.deals {
            // Check VerifiedClient allowed cap and deduct PieceSize from cap.
            // Either the DealSize is within the available DataCap of the VerifiedClient
            // or this message will fail. We do not allow a deal that is partially verified.
            if deal.proposal.verified_deal {
                let ser_params = Serialized::serialize(&BytesParams {
                    address: deal.proposal.client,
                    deal_size: BigUint::from(deal.proposal.piece_size.0),
                })?;
                rt.send(
                    &*VERIFIED_REGISTRY_ACTOR_ADDR,
                    VerifregMethod::UseBytes as u64,
                    &ser_params,
                    &TokenAmount::zero(),
                )?;
            }
        }

        // All deals should have the same provider so get worker once
        let provider_raw = params.deals[0].proposal.provider;
        let provider = rt.resolve_address(&provider_raw)?;

        let mut new_deal_ids: Vec<DealID> = Vec::new();
        rt.transaction(|st: &mut State, rt| {
            let mut prop = Amt::load(&st.proposals, rt.store())
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
            let mut deal_ops = SetMultimap::from_root(rt.store(), &st.deal_ops_by_epoch)
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

                deal_ops
                    .put(deal.proposal.start_epoch, id)
                    .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e))?;

                prop.set(id, deal.proposal)
                    .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

                new_deal_ids.push(id);
            }
            st.proposals = prop
                .flush()
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
            st.deal_ops_by_epoch = deal_ops
                .root()
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

            Ok(())
        })??;

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
    ) -> Result<VerifyDealsOnSectorProveCommitReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        let miner_addr = *rt.message().from();
        let mut total_deal_space_time = BigUint::zero();
        let mut total_verified_deal_space_time = BigUint::zero();
        rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
            // if there are no dealIDs, it is a CommittedCapacity sector
            // and the totalDealSpaceTime should be zero
            let mut states = Amt::load(&st.states, rt.store())
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
            let proposals = Amt::load(&st.proposals, rt.store())
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

            for id in &params.deal_ids {
                let deal = states
                    .get(*id)
                    .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

                if deal.is_some() {
                    // Sector is currently precommitted but still not proven.
                    return Err(ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        format!("given deal already included in another sector: {}", id),
                    ));
                };

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
                    &proposal,
                )?;

                states
                    .set(
                        *id,
                        DealState {
                            sector_start_epoch: rt.curr_epoch(),
                            last_updated_epoch: EPOCH_UNDEFINED,
                            slash_epoch: EPOCH_UNDEFINED,
                        },
                    )
                    .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

                // compute deal weight
                let deal_space_time = proposal.duration() as u64 * proposal.piece_size.0;
                if proposal.verified_deal {
                    total_verified_deal_space_time += deal_space_time;
                } else {
                    total_deal_space_time += deal_space_time;
                }

                if proposal.verified_deal {
                    total_verified_deal_space_time += deal_space_time;
                } else {
                    total_deal_space_time += deal_space_time;
                }
            }
            st.states = states
                .flush()
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;
            Ok(())
        })??;

        Ok(VerifyDealsOnSectorProveCommitReturn {
            deal_weight: total_deal_space_time,
            verified_deal_weight: total_verified_deal_space_time,
        })
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

                state.slash_epoch = rt.curr_epoch();
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

    fn cron_tick<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*CRON_ACTOR_ADDR))?;

        let mut amount_slashed = BigUint::zero();
        let mut timed_out_verified_deals: Vec<DealProposal> = Vec::new();

        rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
            let mut dbe =
                SetMultimap::from_root(rt.store(), &st.deal_ops_by_epoch).map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to load deal opts set: {}", e),
                    )
                })?;

            let mut updates_needed: Vec<(ChainEpoch, DealID)> = Vec::new();

            let mut states = Amt::load(&st.states, rt.store())
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

            let mut et = BalanceTable::from_root(rt.store(), &st.escrow_table)
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

            let mut lt = BalanceTable::from_root(rt.store(), &st.locked_table)
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

            let mut i = st.last_cron + 1;
            while i <= rt.curr_epoch() {
                dbe.for_each(i, |id| {
                    let mut state: DealState = states
                        .get(id)
                        .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?
                        .ok_or_else(|| {
                            ActorError::new(
                                ExitCode::ErrIllegalState,
                                format!("could not find deal state: {}", id),
                            )
                        })?;

                    let deal = st.must_get_deal(rt.store(), id)?;
                    // Not yet appeared in proven sector; check for timeout.
                    if state.sector_start_epoch == EPOCH_UNDEFINED {
                        assert!(
                            rt.curr_epoch() >= deal.start_epoch,
                            "if sector start is not set, we must be in a timed out state"
                        );

                        let slashed = st.process_deal_init_timed_out(
                            rt.store(),
                            &mut et,
                            &mut lt,
                            id,
                            &deal,
                            state,
                        )?;
                        amount_slashed += slashed;

                        if deal.verified_deal {
                            timed_out_verified_deals.push(deal.clone());
                        }
                    }

                    let (slash_amount, next_epoch) = st.update_pending_deal_state(
                        rt.store(),
                        state,
                        deal,
                        id,
                        &mut et,
                        &mut lt,
                        rt.curr_epoch(),
                    )?;
                    amount_slashed += slash_amount;

                    if next_epoch != EPOCH_UNDEFINED {
                        assert!(next_epoch > rt.curr_epoch());

                        // TODO: can we avoid having this field?
                        state.last_updated_epoch = rt.curr_epoch();

                        states.set(id, state).map_err(|e| {
                            ActorError::new(
                                ExitCode::ErrPlaceholder,
                                format!("failed to get deal: {}", e),
                            )
                        })?;
                        updates_needed.push((next_epoch, id));
                    }
                    Ok(())
                })
                .map_err(|e| match e.downcast::<ActorError>() {
                    Ok(actor_err) => *actor_err,
                    Err(other) => ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to iterate deals for epoch: {}", other),
                    ),
                })?;
                dbe.remove_all(i).map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to delete deals from set: {}", e),
                    )
                })?;
                i += 1;
            }

            for (epoch, deals) in updates_needed.into_iter() {
                // TODO multimap should have put_many
                dbe.put(epoch, deals).map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to reinsert deal IDs into epoch set: {}", e),
                    )
                })?;
            }

            let nd_bec = dbe
                .root()
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

            let ltc = lt
                .root()
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

            let etc = et
                .root()
                .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

            st.locked_table = ltc;
            st.escrow_table = etc;

            st.deal_ops_by_epoch = nd_bec;

            st.last_cron = rt.curr_epoch();

            Ok(())
        })??;

        for d in timed_out_verified_deals {
            let ser_params = Serialized::serialize(BytesParams {
                address: d.client,
                deal_size: BigUint::from(d.piece_size.0),
            })?;
            rt.send(
                &*VERIFIED_REGISTRY_ACTOR_ADDR,
                VerifregMethod::RestoreBytes as u64,
                &ser_params,
                &TokenAmount::zero(),
            )?;
        }

        rt.send(
            &*BURNT_FUNDS_ACTOR_ADDR,
            METHOD_SEND,
            &Serialized::default(),
            &amount_slashed,
        )?;
        Ok(())
    }
}
////////////////////////////////////////////////////////////////////////////////
// Checks
////////////////////////////////////////////////////////////////////////////////
fn validate_deal_can_activate(
    curr_epoch: ChainEpoch,
    miner_addr: &Address,
    sector_exp: ChainEpoch,
    proposal: &DealProposal,
) -> Result<(), ActorError> {
    if &proposal.provider != miner_addr {
        return Err(ActorError::new(
            ExitCode::ErrIllegalArgument,
            "Deal has incorrect miner as its provider.".to_owned(),
        ));
    };

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
fn escrow_address<BS, RT>(rt: &mut RT, addr: &Address) -> Result<(Address, Address), ActorError>
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
            Some(Method::PublishStorageDeals) => {
                let res = Self::publish_storage_deals(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::VerifyDealsOnSectorProveCommit) => {
                let res = Self::verify_deals_on_sector_prove_commit(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(&res)?)
            }
            Some(Method::OnMinerSectorsTerminate) => {
                Self::on_miners_sector_terminate(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::ComputeDataCommitment) => {
                let res = Self::compute_data_commitment(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::CronTick) => {
                Self::cron_tick(rt)?;
                Ok(Serialized::default())
            }
            _ => Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method")),
        }
    }
}

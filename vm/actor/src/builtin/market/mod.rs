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
    make_map, request_miner_control_addrs, DealWeight, Multimap, BURNT_FUNDS_ACTOR_ADDR,
    CALLER_TYPES_SIGNABLE, HAMT_BIT_WIDTH, MINER_ACTOR_CODE_ID, SYSTEM_ACTOR_ADDR,
};
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use ipld_hamt::Hamt;
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Zero};
use runtime::Runtime;
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
    ComputeDataCommitment = 7,
}
impl Method {
    /// Converts a method number into a Method enum
    fn _from_method_num(m: MethodNum) -> Option<Method> {
        FromPrimitive::from_u64(m)
    }
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
        let empty_root = Hamt::<String, _>::new_with_bit_width(rt.store(), HAMT_BIT_WIDTH)
            .flush()
            .map_err(|e| {
                rt.abort(
                    ExitCode::ErrIllegalState,
                    format!("Failed to create market actor: {}", e),
                )
            })?;
        let empty_map = make_map(rt.store()).flush().map_err(|err| {
            rt.abort(
                ExitCode::ErrIllegalState,
                format!("Failed to create empty map: {}", err),
            )
        })?;
        let empty_m_set = Multimap::new(rt.store()).root().map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("Failed to construct state: {}", e),
            )
        })?;
        let st = State::new(empty_root, empty_map, empty_m_set);
        rt.create(&st)?;
        Ok(())
    }
    /// Attempt to withdraw the specified amount from the balance held in escrow.
    /// If less than the specified amount is available, yields the entire available balance.
    #[allow(dead_code)]
    pub fn withdraw_balance<BS, RT>(
        &self,
        rt: &mut RT,
        params: WithdrawBalanceParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let (_nominal, _recipient) = escrow_address(rt, params.provider_or_client)?;
        rt.transaction::<State, Result<(), ActorError>, _>(|&mut st, &bs| {
            // do something
            Ok(())
        })??;
        Ok(())
    }

    /// Deposits the received value into the balance held in escrow.
    #[allow(dead_code)]
    fn add_balance<BS, RT>(
        &self,
        rt: &mut RT,
        provider_or_client: Address,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let (nominal, _) = escrow_address(rt, provider_or_client)?;

        rt.transaction::<State, Result<(), ActorError>, _>(|&mut st, &bs| {
            let msg_value = rt.message().value().clone();

            st.add_escrow_balance(&bs, &nominal, msg_value)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("adding to escrow table: {}", e),
                    )
                })?;

            st.add_locked_balance(&bs, &nominal, TokenAmount::zero())
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

    /// Publish a new set of storage deals (not yet included in a sector).
    #[allow(dead_code)]
    fn publish_storage_deals<BS, RT>(
        &self,
        _rt: &mut RT,
        _params: PublishStorageDealsParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        Ok(())
    }

    /// Verify that a given set of storage deals is valid for a sector currently being ProveCommitted,
    /// update the market's internal state accordingly, and return DealWeight of the set of storage deals given.
    /// Note: in the case of a capacity-commitment sector (one with zero deals), this function should succeed vacuously.
    /// The weight is defined as the sum, over all deals in the set, of the product of its size
    /// with its duration. This quantity may be an input into the functions specifying block reward,
    /// sector power, collateral, and/or other parameters.    
    #[allow(dead_code)]
    fn verify_deals_on_sector_prove_commit<BS, RT>(
        &self,
        _rt: &mut RT,
        _params: VerifyDealsOnSectorProveCommitParams,
    ) -> Result<DealWeight, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        todo!()
    }

    #[allow(dead_code)]
    fn compute_data_commitment<BS, RT>(
        &self,
        rt: &mut RT,
        params: ComputeDataCommitmentParams,
    ) -> Result<Cid, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;

        let pieces: Vec<PieceInfo> = Vec::new();
        rt.transaction::<State, Result<(), ActorError>, _>(|&mut st, &bs| {
            for id in params.deal_ids {
                let deal = st.must_get_deal(&bs, id)?;
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
                    ExitCode::ErrIllegalArgument,
                    format!("failed to compute unsealed sector CID: {}", e),
                )
            })?;

        Ok(commd)
    }
    /// Terminate a set of deals in response to their containing sector being terminated.
    /// Slash provider collateral, refund client collateral, and refund partial unpaid escrow
    /// amount to client.    
    #[allow(dead_code)]
    fn on_miners_sector_terminate<BS, RT>(
        &self,
        rt: &mut RT,
        params: OnMinerSectorsTerminateParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;

        let miner_addr = rt.message().from().clone();

        rt.transaction::<State, Result<(), ActorError>, _>(|&mut st, &bs| {
            let prop = Amt::load(&st.proposals, &bs)?;
            let states = Amt::load(&st.states, &bs)?;

            for id in params.deal_ids {
                let deal = match prop.get(id)? {
                    Some(deal) => deal,
                    Err(e) => {
                        return ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("Get deal error: {}", e),
                        )
                    }
                };
                assert_eq!(deal.provider, miner_addr);
                let state = match states.get(id)? {
                    Some(state) => state,
                    Err(e) => {
                        return ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("Get deal error: {}", e),
                        )
                    }
                };
                // Note: we do not perform the balance transfers here, but rather simply record the flag
                // to indicate that processDealSlashed should be called when the deferred state computation
                // is performed. // TODO: Do that here

                state.slash_epoch = rt.curr_epoch();
                states.set(id, state).map_err(|e| {
                    ActorError::new(ExitCode::ErrIllegalState, format!("set deal error: {}", e))
                });
            }
            Ok(())
        })??;
        Ok(())
    }

    #[allow(dead_code)]
    fn handle_expired_deal<BS, RT>(
        &self,
        rt: &mut RT,
        params: HandleExpiredDealsParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;
        let mut slashed = TokenAmount::zero();
        rt.transaction::<State, Result<(), ActorError>, _>(|st: &mut State, bs| {
            slashed = st.update_pending_deal_states(&bs, params.deal_ids, rt.curr_epoch())?;
            Ok(())
        })??;

        // TODO: award some small portion of slashed to caller as incentive
        let _ret = rt.send(
            &*BURNT_FUNDS_ACTOR_ADDR,
            METHOD_SEND,
            &Serialized::default(),
            &slashed,
        )?;
        // TODO investigate if require_success is needed here
        Ok(())
    }
}
////////////////////////////////////////////////////////////////////////////////
// Checks
////////////////////////////////////////////////////////////////////////////////
#[allow(dead_code)]
fn validate_deal_can_activate<BS, RT>(
    rt: &RT,
    miner_addr: Address,
    sector_exp: ChainEpoch,
    proposal: DealProposal,
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if proposal.provider != miner_addr {
        return Err(ActorError::new(
            ExitCode::ErrIllegalArgument,
            "Deal has incorrect miner as its provider.".to_owned(),
        ));
    };

    if rt.curr_epoch() > proposal.start_epoch {
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
#[allow(dead_code)]
fn validate_deal<BS, RT>(rt: &RT, deal: ClientDealProposal) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    // todo deal_proposal_is_internally_valid

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

    let (min_collateral, max_collateral) =
        deal_client_collateral_bounds(deal.proposal.piece_size, deal.proposal.duration());
    if deal.proposal.provider_collateral < min_collateral
        || deal.proposal.provider_collateral > max_collateral
    {
        return Err(ActorError::new(
            ExitCode::ErrIllegalArgument,
            "Client collateral out of bounds.".to_owned(),
        ));
    };

    Ok(())
}

// Resolves a provider or client address to the canonical form against which a balance should be held, and
// the designated recipient address of withdrawals (which is the same, for simple account parties).
pub fn escrow_address<BS, RT>(rt: &RT, addr: Address) -> Result<(Address, Address), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let nominal = rt.resolve_address(&addr)?;

    let code_id = rt.get_actor_code_cid(&nominal)?;

    if code_id != *MINER_ACTOR_CODE_ID {
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;
        return Ok((nominal.clone(), nominal));
    }

    let (owner_addr, worker_addr) = request_miner_control_addrs(rt, &nominal)?;
    rt.validate_immediate_caller_is([owner_addr.clone(), worker_addr].iter())?;
    Ok((nominal, owner_addr))
}

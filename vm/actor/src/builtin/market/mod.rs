// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod deal;
mod policy;
mod state;
mod types;

pub use self::deal::*;
use self::policy::*;
pub use self::state::*;
pub use self::types::*;
use crate::{
    check_empty_params, make_map, power, request_miner_control_addrs, reward,
    verifreg::{Method as VerifregMethod, UseBytesParams},
    BalanceTable, DealID, SetMultimap, BURNT_FUNDS_ACTOR_ADDR, CALLER_TYPES_SIGNABLE,
    CRON_ACTOR_ADDR, MINER_ACTOR_CODE_ID, REWARD_ACTOR_ADDR, STORAGE_POWER_ACTOR_ADDR,
    SYSTEM_ACTOR_ADDR, VERIFIED_REGISTRY_ACTOR_ADDR,
};
use address::Address;
use cid::Cid;
use clock::{ChainEpoch, EPOCH_UNDEFINED};
use encoding::{to_vec, Cbor};
use fil_types::{PieceInfo, StoragePower};
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use num_bigint::BigInt;
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Zero};
use runtime::{ActorCode, Runtime};
use std::collections::HashMap;
use vm::{
    actor_error, ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR,
    METHOD_SEND,
};

// * Updated to specs-actors commit: 4784ddb8e54d53c118e63763e4efbcf0a419da28

/// Market actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    AddBalance = 2,
    WithdrawBalance = 3,
    PublishStorageDeals = 4,
    VerifyDealsForActivation = 5,
    ActivateDeals = 6,
    OnMinerSectorsTerminate = 7,
    ComputeDataCommitment = 8,
    CronTick = 9,
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

        let empty_root = Amt::<(), BS>::new(rt.store())
            .flush()
            .map_err(|e| actor_error!(ErrIllegalState; "Failed to create market state: {}", e))?;

        let empty_map = make_map(rt.store())
            .flush()
            .map_err(|e| actor_error!(ErrIllegalState; "Failed to create market state: {}", e))?;

        let empty_m_set = SetMultimap::new(rt.store())
            .root()
            .map_err(|e| actor_error!(ErrIllegalState; "Failed to create market state: {}", e))?;

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
        let msg_value = rt.message().value_received().clone();

        if msg_value <= TokenAmount::from(0) {
            return Err(actor_error!(ErrIllegalArgument;
                "balance to add must be greater than zero was: {}", msg_value));
        }

        // only signing parties can add balance for client AND provider.
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;

        let (nominal, _, _) = escrow_address(rt, &provider_or_client)?;

        rt.transaction::<State, Result<_, ActorError>, _>(|st, rt| {
            let mut msm = st.mutator(rt.store());
            msm.with_escrow_table(Permission::Write)
                .with_locked_table(Permission::Write)
                .build()
                .map_err(|e| actor_error!(ErrIllegalState; "failed to load state: {}", e))?;

            msm.escrow_table
                .as_mut()
                .unwrap()
                .add(&nominal, &msg_value)
                .map_err(|e| {
                    actor_error!(ErrIllegalState;
                            "failed to add balance to escrow table: {}", e)
                })?;

            msm.commit_state()
                .map_err(|e| actor_error!(ErrIllegalState; "failed to flush state: {}", e))?;

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
        if params.amount < TokenAmount::from(0) {
            return Err(actor_error!(ErrIllegalArgument; "negative amount: {}", params.amount));
        }
        // withdrawal can ONLY be done by a signing party.
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;

        let (nominal, recipient, approved) = escrow_address(rt, &params.provider_or_client)?;
        // for providers -> only corresponding owner or worker can withdraw
        // for clients -> only the client i.e the recipient can withdraw
        rt.validate_immediate_caller_is(&approved)?;

        let amount_extracted =
            rt.transaction::<State, Result<TokenAmount, ActorError>, _>(|st, rt| {
                let mut msm = st.mutator(rt.store());
                msm.with_escrow_table(Permission::Write)
                    .with_locked_table(Permission::Write)
                    .build()
                    .map_err(|e| actor_error!(ErrIllegalState; "failed to load state: {}", e))?;

                // The withdrawable amount might be slightly less than nominal
                // depending on whether or not all relevant entries have been processed
                // by cron

                let min_balance = msm.locked_table.as_ref().unwrap().get(&nominal).map_err(
                    |e| actor_error!(ErrIllegalState; "failed to get locked balance: {}", e),
                )?;

                let ex = msm
                    .escrow_table
                    .as_mut()
                    .unwrap()
                    .subtract_with_minimum(&nominal, &params.amount, &min_balance)
                    .map_err(|e| {
                        actor_error!(ErrIllegalState;
                            "failed to subtract from escrow table: {}", e)
                    })?;

                msm.commit_state()
                    .map_err(|e| actor_error!(ErrIllegalState; "failed to flush state: {}", e))?;

                Ok(ex)
            })??;

        rt.send(
            recipient,
            METHOD_SEND,
            Serialized::default(),
            amount_extracted,
        )?;
        Ok(())
    }

    /// Publish a new set of storage deals (not yet included in a sector).
    fn publish_storage_deals<BS, RT>(
        rt: &mut RT,
        mut params: PublishStorageDealsParams,
    ) -> Result<PublishStorageDealsReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // Deal message must have a From field identical to the provider of all the deals.
        // This allows us to retain and verify only the client's signature in each deal proposal itself.
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;
        if params.deals.is_empty() {
            return Err(actor_error!(ErrIllegalArgument; "Empty deals parameter"));
        }

        // All deals should have the same provider so get worker once
        let provider_raw = params.deals[0].proposal.provider;
        let provider = rt.resolve_address(&provider_raw)?.ok_or_else(
            || actor_error!(ErrNotFound; "failed to resolve provider address {}", provider_raw),
        )?;

        let code_id = rt.get_actor_code_cid(&provider)?.ok_or_else(
            || actor_error!(ErrIllegalArgument; "no code ID for address {}", provider),
        )?;
        if code_id != *MINER_ACTOR_CODE_ID {
            return Err(
                actor_error!(ErrIllegalArgument; "deal provider is not a storage miner actor"),
            );
        }

        let (_, worker) = request_miner_control_addrs(rt, provider)?;
        if &worker != rt.message().caller() {
            return Err(actor_error!(ErrForbidden; "Caller is not provider {}", worker));
        }

        let mut resolved_addrs = HashMap::<Address, Address>::with_capacity(params.deals.len());
        let baseline_power = request_current_baseline_power(rt)?;
        let network_qa_power = request_current_network_qa_power(rt)?;

        let mut new_deal_ids: Vec<DealID> = Vec::new();
        rt.transaction(|st: &mut State, rt| {
            let mut msm = st.mutator(rt.store());
            msm.with_pending_proposals(Permission::Write)
                .with_deal_proposals(Permission::Write)
                .with_deals_by_epoch(Permission::Write)
                .with_escrow_table(Permission::Write)
                .with_locked_table(Permission::Write)
                .build()
                .map_err(|e| actor_error!(ErrIllegalState; "failed to load state: {}", e))?;

            for deal in &mut params.deals {
                validate_deal(rt, &deal, &baseline_power, &network_qa_power)?;

                if deal.proposal.provider != provider && deal.proposal.provider != provider_raw {
                    return Err(actor_error!(ErrIllegalArgument;
                        "cannot publish deals from different providers at the same time."));
                }

                let client = rt.resolve_address(&deal.proposal.client)?.ok_or_else(|| {
                    actor_error!(ErrNotFound;
                        "failed to resolve provider address {}", provider_raw)
                })?;
                // Normalise provider and client addresses in the proposal stored on chain (after signature verification).
                deal.proposal.provider = provider;
                resolved_addrs.insert(deal.proposal.client, client);
                deal.proposal.client = client;

                msm.lock_client_and_provider_balances(&deal.proposal)?;

                let id = msm.generate_storage_deal_id();

                let pcid = deal.proposal.cid().map_err(
                    |e| actor_error!(ErrIllegalArgument; "failed to take cid of proposal: {}", e),
                )?;

                let has = msm
                    .pending_deals
                    .as_ref()
                    .unwrap()
                    .contains_key(&pcid.to_bytes())
                    .map_err(|e| {
                        actor_error!(ErrIllegalState;
                        "failed to check for existence of deal proposal: {}", e)
                    })?;
                if has {
                    return Err(actor_error!(ErrIllegalArgument; "cannot publish duplicate deals"));
                }

                msm.pending_deals
                    .as_mut()
                    .unwrap()
                    .set(pcid.to_bytes().into(), deal.proposal.clone())
                    .map_err(
                        |e| actor_error!(ErrIllegalState; "failed to set pending deal: {}", e),
                    )?;
                msm.deal_proposals
                    .as_mut()
                    .unwrap()
                    .set(id, deal.proposal.clone())
                    .map_err(|e| actor_error!(ErrIllegalState; "failed to set deal: {}", e))?;
                msm.deals_by_epoch
                    .as_mut()
                    .unwrap()
                    .put(deal.proposal.start_epoch, id)
                    .map_err(
                        |e| actor_error!(ErrIllegalState; "failed to set deal ops by epoch: {}", e),
                    )?;

                new_deal_ids.push(id);
            }

            msm.commit_state()
                .map_err(|e| actor_error!(ErrIllegalState; "failed to flush state: {}", e))?;
            Ok(())
        })??;

        for deal in &params.deals {
            // Check VerifiedClient allowed cap and deduct PieceSize from cap.
            // Either the DealSize is within the available DataCap of the VerifiedClient
            // or this message will fail. We do not allow a deal that is partially verified.
            if deal.proposal.verified_deal {
                let resolved_client = resolved_addrs.get(&deal.proposal.client).ok_or_else(
                    || actor_error!(ErrIllegalArgument; "could not get resolved client address"),
                )?;
                rt.send(
                    *VERIFIED_REGISTRY_ACTOR_ADDR,
                    VerifregMethod::UseBytes as u64,
                    Serialized::serialize(&UseBytesParams {
                        address: deal.proposal.client,
                        deal_size: BigInt::from(deal.proposal.piece_size.0),
                    })?,
                    TokenAmount::zero(),
                )
                .map_err(|e| {
                    e.wrap(&format!(
                        "failed to add verified deal for client ({}): ",
                        deal.proposal.client
                    ))
                })?;
            }
        }

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
        params: VerifyDealsForActivationParams,
    ) -> Result<VerifyDealsForActivationReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        let miner_addr = *rt.message().caller();
        let mut total_deal_space_time = BigInt::zero();
        let mut total_verified_deal_space_time = BigInt::zero();
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

        Ok(VerifyDealsForActivationReturn {
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
        let miner_addr = *rt.message().caller();

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
        todo!();
        // rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
        //     for id in &params.deal_ids {
        //         let deal = st.must_get_deal(rt.store(), *id)?;
        //         pieces.push(PieceInfo {
        //             size: deal.piece_size,
        //             cid: deal.piece_cid,
        //         });
        //     }
        //     Ok(())
        // })??;

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

        let mut amount_slashed = BigInt::zero();
        let mut timed_out_verified_deals: Vec<DealProposal> = Vec::new();

        todo!();
        // rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
        //     let mut dbe =
        //         SetMultimap::from_root(rt.store(), &st.deal_ops_by_epoch).map_err(|e| {
        //             ActorError::new(
        //                 ExitCode::ErrIllegalState,
        //                 format!("failed to load deal opts set: {}", e),
        //             )
        //         })?;

        //     let mut updates_needed: Vec<(ChainEpoch, DealID)> = Vec::new();

        //     let mut states = Amt::load(&st.states, rt.store())
        //         .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

        //     let mut et = BalanceTable::from_root(rt.store(), &st.escrow_table)
        //         .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

        //     let mut lt = BalanceTable::from_root(rt.store(), &st.locked_table)
        //         .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

        //     let mut i = st.last_cron + 1;
        //     while i <= rt.curr_epoch() {
        //         dbe.for_each(i, |id| {
        //             let mut state: DealState = states
        //                 .get(id)
        //                 .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?
        //                 .ok_or_else(|| {
        //                     ActorError::new(
        //                         ExitCode::ErrIllegalState,
        //                         format!("could not find deal state: {}", id),
        //                     )
        //                 })?;

        //             let deal = st.must_get_deal(rt.store(), id)?;
        //             // Not yet appeared in proven sector; check for timeout.
        //             if state.sector_start_epoch == EPOCH_UNDEFINED {
        //                 assert!(
        //                     rt.curr_epoch() >= deal.start_epoch,
        //                     "if sector start is not set, we must be in a timed out state"
        //                 );

        //                 let slashed = st.process_deal_init_timed_out(
        //                     rt.store(),
        //                     &mut et,
        //                     &mut lt,
        //                     id,
        //                     &deal,
        //                     state,
        //                 )?;
        //                 amount_slashed += slashed;

        //                 if deal.verified_deal {
        //                     timed_out_verified_deals.push(deal.clone());
        //                 }
        //             }

        //             let (slash_amount, next_epoch) = st.update_pending_deal_state(
        //                 rt.store(),
        //                 state,
        //                 deal,
        //                 id,
        //                 &mut et,
        //                 &mut lt,
        //                 rt.curr_epoch(),
        //             )?;
        //             amount_slashed += slash_amount;

        //             if next_epoch != EPOCH_UNDEFINED {
        //                 assert!(next_epoch > rt.curr_epoch());

        //                 // TODO: can we avoid having this field?
        //                 state.last_updated_epoch = rt.curr_epoch();

        //                 states.set(id, state).map_err(|e| {
        //                     ActorError::new(
        //                         ExitCode::ErrPlaceholder,
        //                         format!("failed to get deal: {}", e),
        //                     )
        //                 })?;
        //                 updates_needed.push((next_epoch, id));
        //             }
        //             Ok(())
        //         })
        //         .map_err(|e| match e.downcast::<ActorError>() {
        //             Ok(actor_err) => *actor_err,
        //             Err(other) => ActorError::new(
        //                 ExitCode::ErrIllegalState,
        //                 format!("failed to iterate deals for epoch: {}", other),
        //             ),
        //         })?;
        //         dbe.remove_all(i).map_err(|e| {
        //             ActorError::new(
        //                 ExitCode::ErrIllegalState,
        //                 format!("failed to delete deals from set: {}", e),
        //             )
        //         })?;
        //         i += 1;
        //     }

        //     for (epoch, deals) in updates_needed.into_iter() {
        //         // TODO multimap should have put_many
        //         dbe.put(epoch, deals).map_err(|e| {
        //             ActorError::new(
        //                 ExitCode::ErrIllegalState,
        //                 format!("failed to reinsert deal IDs into epoch set: {}", e),
        //             )
        //         })?;
        //     }

        //     let nd_bec = dbe
        //         .root()
        //         .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

        //     let ltc = lt
        //         .root()
        //         .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

        //     let etc = et
        //         .root()
        //         .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.into()))?;

        //     st.locked_table = ltc;
        //     st.escrow_table = etc;

        //     st.deal_ops_by_epoch = nd_bec;

        //     st.last_cron = rt.curr_epoch();

        //     Ok(())
        // })??;

        // for d in timed_out_verified_deals {
        //     let ser_params = Serialized::serialize(UseBytesParams {
        //         address: d.client,
        //         deal_size: BigInt::from(d.piece_size.0),
        //     })?;
        //     rt.send(
        //         *VERIFIED_REGISTRY_ACTOR_ADDR,
        //         VerifregMethod::RestoreBytes as u64,
        //         ser_params,
        //         TokenAmount::zero(),
        //     )?;
        // }

        rt.send(
            *BURNT_FUNDS_ACTOR_ADDR,
            METHOD_SEND,
            Serialized::default(),
            amount_slashed,
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

fn validate_deal<BS, RT>(
    rt: &RT,
    deal: &ClientDealProposal,
    baseline_power: &StoragePower,
    network_qa_power: &StoragePower,
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    deal_proposal_is_internally_valid(rt, deal)?;

    let proposal = &deal.proposal;

    proposal
        .piece_size
        .validate()
        .map_err(|e| actor_error!(ErrIllegalArgument; "proposal piece size is invalid: {}", e))?;

    // TODO we are skipping the check for if Cid is defined, but this shouldn't be possible

    if proposal.piece_cid.prefix() != PIECE_CID_PREFIX {
        return Err(actor_error!(ErrIllegalArgument; "proposal PieceCID undefined"));
    }

    if proposal.end_epoch <= proposal.start_epoch {
        return Err(actor_error!(ErrIllegalArgument; "proposal end before start"));
    }

    if rt.curr_epoch() > proposal.start_epoch {
        return Err(actor_error!(ErrIllegalArgument; "Deal start epoch has already elapsed."));
    };

    let (min_dur, max_dur) = deal_duration_bounds(proposal.piece_size);
    if proposal.duration() < min_dur || proposal.duration() > max_dur {
        return Err(actor_error!(ErrIllegalArgument; "Deal duration out of bounds."));
    };

    let (min_price, max_price) =
        deal_price_per_epoch_bounds(proposal.piece_size, proposal.duration());
    if proposal.storage_price_per_epoch < min_price || proposal.storage_price_per_epoch > max_price
    {
        return Err(actor_error!(ErrIllegalArgument; "Storage price out of bounds."));
    };

    let (min_provider_collateral, max_provider_collateral) = deal_provider_collateral_bounds(
        proposal.piece_size,
        proposal.verified_deal,
        network_qa_power,
        baseline_power,
        &rt.total_fil_circ_supply()?,
    );
    if proposal.provider_collateral < min_provider_collateral
        || proposal.provider_collateral > max_provider_collateral
    {
        return Err(actor_error!(ErrIllegalArgument; "Provider collateral out of bounds."));
    };

    let (min_client_collateral, max_client_collateral) =
        deal_client_collateral_bounds(proposal.piece_size, proposal.duration());
    if proposal.provider_collateral < min_client_collateral
        || proposal.provider_collateral > max_client_collateral
    {
        return Err(actor_error!(ErrIllegalArgument; "Client collateral out of bounds."));
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
    let sv_bz = to_vec(&proposal.proposal)
        .map_err(|_| actor_error!(ErrIllegalArgument; "failed to serialize DealProposal"))?;

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
fn escrow_address<BS, RT>(
    rt: &mut RT,
    addr: &Address,
) -> Result<(Address, Address, Vec<Address>), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    // Resolve the provided address to the canonical form against which the balance is held.
    let nominal = rt
        .resolve_address(addr)?
        .ok_or_else(|| actor_error!(ErrIllegalArgument; "failed to resolve address {}", addr))?;

    let code_id = rt
        .get_actor_code_cid(&nominal)?
        .ok_or_else(|| actor_error!(ErrIllegalArgument; "no code for address {}", nominal))?;

    if code_id == *MINER_ACTOR_CODE_ID {
        // Storage miner actor entry; implied funds recipient is the associated owner address.
        let (owner_addr, worker_addr) = request_miner_control_addrs(rt, nominal)?;
        return Ok((nominal, owner_addr, vec![owner_addr, worker_addr]));
    }

    Ok((nominal, nominal, vec![nominal]))
}

// Requests the current epoch target block reward from the reward actor.
fn request_current_baseline_power<BS, RT>(rt: &mut RT) -> Result<StoragePower, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let rwret = rt.send(
        *REWARD_ACTOR_ADDR,
        reward::Method::ThisEpochReward as u64,
        Serialized::default(),
        0.into(),
    )?;
    let ret: reward::ThisEpochRewardReturn = rwret.deserialize()?;
    Ok(ret.this_epoch_baseline_power)
}

// Requests the current network total power and pledge from the power actor.
fn request_current_network_qa_power<BS, RT>(rt: &mut RT) -> Result<StoragePower, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let rwret = rt.send(
        *STORAGE_POWER_ACTOR_ADDR,
        power::Method::CurrentTotalPower as u64,
        Serialized::default(),
        0.into(),
    )?;
    let ret: power::CurrentTotalPowerReturn = rwret.deserialize()?;
    Ok(ret.quality_adj_power)
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
                check_empty_params(params)?;
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
            Some(Method::VerifyDealsForActivation) => todo!(),
            Some(Method::ActivateDeals) => todo!(),
            Some(Method::OnMinerSectorsTerminate) => {
                Self::on_miners_sector_terminate(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::ComputeDataCommitment) => {
                let res = Self::compute_data_commitment(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::CronTick) => {
                check_empty_params(params)?;
                Self::cron_tick(rt)?;
                Ok(Serialized::default())
            }
            None => Err(actor_error!(SysErrInvalidMethod; "Invalid method")),
        }
    }
}

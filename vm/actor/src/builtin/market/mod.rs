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
    verifreg::{Method as VerifregMethod, RestoreBytesParams, UseBytesParams},
    DealID, SetMultimap, BURNT_FUNDS_ACTOR_ADDR, CALLER_TYPES_SIGNABLE, CRON_ACTOR_ADDR,
    MINER_ACTOR_CODE_ID, REWARD_ACTOR_ADDR, STORAGE_POWER_ACTOR_ADDR, SYSTEM_ACTOR_ADDR,
    VERIFIED_REGISTRY_ACTOR_ADDR,
};
use address::Address;
use ahash::AHashMap;
use byteorder::{BigEndian, ByteOrder};
use cid::Cid;
use clock::{ChainEpoch, EPOCH_UNDEFINED};
use crypto::DomainSeparationTag;
use encoding::{to_vec, Cbor};
use fil_types::{PieceInfo, StoragePower};
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use num_bigint::{BigInt, Sign};
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Zero};
use runtime::{ActorCode, Runtime};
use std::collections::HashMap;
use std::error::Error as StdError;
use vm::{
    actor_error, ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR,
    METHOD_SEND,
};

// * Updated to specs-actors commit: f4024efad09a66e32bfeef10a2845b2b35325297 (v0.9.3)

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

        let (_, worker, _) = request_miner_control_addrs(rt, provider)?;
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
                    actor_error!(ErrNotFound; "failed to resolve client address {}", provider_raw)
                })?;
                // Normalise provider and client addresses in the proposal stored on chain
                // (after signature verification).
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

                // We should randomize the first epoch for when the deal will be processed so an attacker isn't able to
                // schedule too many deals for the same tick.
                let process_epoch = gen_rand_next_epoch(rt, rt.curr_epoch(), &deal.proposal)
                    .map_err(|e| {
                        ActorError::downcast(
                            e,
                            ExitCode::ErrIllegalState,
                            "failed to generate random process epoch",
                        )
                    })?;

                msm.deals_by_epoch
                    .as_mut()
                    .unwrap()
                    .put(process_epoch, id)
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
                let resolved_client = *resolved_addrs.get(&deal.proposal.client).ok_or_else(
                    || actor_error!(ErrIllegalArgument; "could not get resolved client address"),
                )?;
                rt.send(
                    *VERIFIED_REGISTRY_ACTOR_ADDR,
                    VerifregMethod::UseBytes as u64,
                    Serialized::serialize(&UseBytesParams {
                        address: resolved_client,
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

    /// Verify that a given set of storage deals is valid for a sector currently being PreCommitted
    /// and return DealWeight of the set of storage deals given.
    /// The weight is defined as the sum, over all deals in the set, of the product of deal size
    /// and duration.
    fn verify_deals_for_activation<BS, RT>(
        rt: &mut RT,
        params: VerifyDealsForActivationParams,
    ) -> Result<VerifyDealsForActivationReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        let miner_addr = *rt.message().caller();

        let st: State = rt.state()?;

        let (deal_weight, verified_deal_weight) = validate_deals_for_activation(
            &st,
            rt.store(),
            &params.deal_ids,
            &miner_addr,
            params.sector_expiry,
            params.sector_start,
        )
        .map_err(|e| match e.downcast::<ActorError>() {
            Ok(actor_err) => *actor_err,
            Err(other) => actor_error!(ErrIllegalState;
                "failed to validate deal proposals for activation: {}", other),
        })?;

        Ok(VerifyDealsForActivationReturn {
            deal_weight,
            verified_deal_weight,
        })
    }

    /// Verify that a given set of storage deals is valid for a sector currently being ProveCommitted,
    /// update the market's internal state accordingly.
    fn activate_deals<BS, RT>(rt: &mut RT, params: ActivateDealsParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        let miner_addr = *rt.message().caller();
        let curr_epoch = rt.curr_epoch();

        // Update deal states
        rt.transaction(|st: &mut State, rt| {
            validate_deals_for_activation(
                &st,
                rt.store(),
                &params.deal_ids,
                &miner_addr,
                params.sector_expiry,
                curr_epoch,
            )
            .map_err(|e| match e.downcast::<ActorError>() {
                Ok(actor_err) => *actor_err,
                Err(other) => actor_error!(ErrIllegalState;
                    "failed to validate deal proposals for activation: {}", other),
            })?;

            let mut msm = st.mutator(rt.store());
            msm.with_deal_states(Permission::Write)
                .with_pending_proposals(Permission::ReadOnly)
                .with_deal_proposals(Permission::ReadOnly)
                .build()
                .map_err(|e| actor_error!(ErrIllegalState; "failed to load state: {}", e))?;

            for deal_id in params.deal_ids {
                // This construction could be replaced with a single "update deal state"
                // state method, possibly batched over all deal ids at once.
                let s = msm
                    .deal_states
                    .as_ref()
                    .unwrap()
                    .get(deal_id)
                    .map_err(|e| {
                        actor_error!(ErrIllegalState;
                        "failed to get state for deal_id ({}): {}", deal_id, e)
                    })?;
                if s.is_some() {
                    return Err(actor_error!(ErrIllegalArgument;
                        "deal {} already included in another sector", deal_id));
                }

                let proposal = msm
                    .deal_proposals
                    .as_ref()
                    .unwrap()
                    .get(deal_id)
                    .map_err(|e| {
                        actor_error!(ErrIllegalState;
                            "failed to get deal_id ({}): {}", deal_id, e)
                    })?
                    .ok_or_else(|| actor_error!(ErrNotFound; "no such deal_id: {}", deal_id))?;

                let propc = proposal.cid().map_err(|e| {
                    actor_error!(ErrIllegalState;
                        "failed to calculate proposal CID: {}", e)
                })?;

                let has = msm
                    .pending_deals
                    .as_ref()
                    .unwrap()
                    .contains_key(&propc.to_bytes())
                    .map_err(|e| {
                        actor_error!(ErrIllegalState;
                            "failed to get pending proposal ({}): {}", propc, e)
                    })?;

                if !has {
                    return Err(actor_error!(ErrIllegalState;
                        "tried to activate deal that was not in the pending set ({})", propc));
                }

                msm.deal_states
                    .as_mut()
                    .unwrap()
                    .set(
                        deal_id,
                        DealState {
                            sector_start_epoch: curr_epoch,
                            last_updated_epoch: EPOCH_UNDEFINED,
                            slash_epoch: EPOCH_UNDEFINED,
                        },
                    )
                    .map_err(|e| {
                        actor_error!(ErrIllegalState;
                            "failed to set deal state {}: {}", deal_id, e)
                    })?;
            }

            msm.commit_state()
                .map_err(|e| actor_error!(ErrIllegalState; "failed to flush state: {}", e))?;
            Ok(())
        })??;

        Ok(())
    }

    /// Terminate a set of deals in response to their containing sector being terminated.
    /// Slash provider collateral, refund client collateral, and refund partial unpaid escrow
    /// amount to client.
    fn on_miner_sectors_terminate<BS, RT>(
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
            let mut msm = st.mutator(rt.store());
            msm.with_deal_states(Permission::Write)
                .with_deal_proposals(Permission::ReadOnly)
                .build()
                .map_err(|e| actor_error!(ErrIllegalState; "failed to load state: {}", e))?;

            for id in params.deal_ids {
                let deal = msm.deal_proposals.as_ref().unwrap().get(id).map_err(
                    |e| actor_error!(ErrIllegalState; "failed to get deal proposal: {}", e),
                )?;
                // deal could have terminated and hence deleted before the sector is terminated.
                // we should simply continue instead of aborting execution here if a deal is not found.
                if deal.is_none() {
                    continue;
                }
                let deal = deal.unwrap();

                assert_eq!(
                    deal.provider, miner_addr,
                    "caller is not the provider of the deal"
                );

                // do not slash expired deals
                if deal.end_epoch <= params.epoch {
                    continue;
                }

                let mut state: DealState = msm
                    .deal_states
                    .as_ref()
                    .unwrap()
                    .get(id)
                    .map_err(|e| actor_error!(ErrIllegalState; "failed to get deal state {}", e))?
                    .ok_or_else(|| actor_error!(ErrIllegalArgument; "no state for deal {}", id))?;

                // If a deal is already slashed, don't need to do anything
                if state.slash_epoch != EPOCH_UNDEFINED {
                    continue;
                }

                // mark the deal for slashing here. Actual releasing of locked funds for the client
                // and slashing of provider collateral happens in cron_tick.
                state.slash_epoch = params.epoch;

                msm.deal_states.as_mut().unwrap().set(id, state).map_err(
                    |e| actor_error!(ErrIllegalState; "failed to set deal state ({}): {}", id, e),
                )?;
            }

            msm.commit_state()
                .map_err(|e| actor_error!(ErrIllegalState; "failed to flush state: {}", e))?;
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

        let st: State = rt.state()?;

        let proposals = DealArray::load(&st.proposals, rt.store())
            .map_err(|e| actor_error!(ErrIllegalState; "failed to load deal proposals: {}", e))?;

        let mut pieces: Vec<PieceInfo> = Vec::with_capacity(params.deal_ids.len());
        for deal_id in params.deal_ids {
            let deal = proposals
                .get(deal_id)
                .map_err(
                    |e| actor_error!(ErrIllegalState; "failed to get deal_id ({}): {}", deal_id, e),
                )?
                .ok_or_else(|| actor_error!(ErrNotFound; "proposal doesn't exist ({})", deal_id))?;

            pieces.push(PieceInfo {
                cid: deal.piece_cid,
                size: deal.piece_size,
            });
        }

        let commd = rt
            .syscalls()
            .compute_unsealed_sector_cid(params.sector_type, &pieces)
            .map_err(|e| {
                actor_error!(SysErrorIllegalArgument;
                    "failed to compute unsealed sector CID: {}", e)
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
        let curr_epoch = rt.curr_epoch();
        let mut timed_out_verified_deals: Vec<DealProposal> = Vec::new();

        rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
            let last_cron = st.last_cron;
            let mut updates_needed: AHashMap<ChainEpoch, Vec<DealID>> = AHashMap::new();
            let mut msm = st.mutator(rt.store());
            msm.with_deal_states(Permission::Write)
                .with_locked_table(Permission::Write)
                .with_escrow_table(Permission::Write)
                .with_deals_by_epoch(Permission::Write)
                .with_deal_proposals(Permission::Write)
                .with_pending_proposals(Permission::Write)
                .build()
                .map_err(|e| actor_error!(ErrIllegalState; "failed to load state: {}", e))?;

            for i in (last_cron + 1)..rt.curr_epoch() {
                // TODO specs-actors modifies msm as it's iterated through, which is memory unsafe
                // for now the deal ids are being collected and then iterated on, which could
                // cause a potential inconsistency in exit code returned if a deal_id fails
                // to be pulled from storage where it wouldn't be triggered otherwise.
                // Workaround a better solution (seperating msm or fixing go impl)
                let mut deal_ids = Vec::new();
                msm.deals_by_epoch
                    .as_ref()
                    .unwrap()
                    .for_each(i, |deal_id| {
                        deal_ids.push(deal_id);
                        Ok(())
                    })
                    .map_err(
                        |e| actor_error!(ErrIllegalState; "failed to set deal state: {}", e),
                    )?;

                for deal_id in deal_ids {
                    let deal = msm
                        .deal_proposals
                        .as_ref()
                        .unwrap()
                        .get(deal_id)
                        .map_err(|e| {
                            actor_error!(ErrIllegalState;
                                        "failed to get deal_id ({}): {}", deal_id, e)
                        })?
                        .ok_or_else(|| {
                            actor_error!(ErrNotFound;
                                    "proposal doesn't exist ({})", deal_id)
                        })?;

                    let dcid = deal.cid().map_err(|e| {
                        actor_error!(ErrIllegalState;
                                    "failed to calculate cid for proposal {}: {}", deal_id, e)
                    })?;

                    let state = msm.deal_states.as_ref().unwrap().get(deal_id).map_err(
                        |e| actor_error!(ErrIllegalState; "failed to get deal state: {}", e),
                    )?;

                    // deal has been published but not activated yet -> terminate it
                    // as it has timed out
                    if state.is_none() {
                        // Not yet appeared in proven sector; check for timeout.
                        assert!(
                            curr_epoch >= deal.start_epoch,
                            "if sector start is not set, must be in timed out state"
                        );

                        let slashed = msm.process_deal_init_timed_out(&deal)?;
                        if !slashed.is_zero() {
                            amount_slashed += slashed;
                        }
                        if deal.verified_deal {
                            timed_out_verified_deals.push(deal.clone());
                        }

                        // we should not attempt to delete the DealState because it does NOT exist
                        let deleted = msm
                            .deal_proposals
                            .as_mut()
                            .unwrap()
                            .delete(deal_id)
                            .map_err(
                                |e| actor_error!(ErrIllegalState; "failed to delete deal: {}", e),
                            )?;
                        if !deleted {
                            return Err(actor_error!(ErrIllegalState;
                                        "failed to delete deal proposal: does not exist"));
                        }
                        let deleted = msm
                            .pending_deals
                            .as_mut()
                            .unwrap()
                            .delete(&dcid.to_bytes())
                            .map_err(|e| {
                                actor_error!(ErrIllegalState;
                                "failed to delete pending proposal: {}", e)
                            })?;
                        if !deleted {
                            return Err(actor_error!(ErrIllegalState;
                                            "failed to delete pending proposal: does not exist"));
                        }
                    }
                    let mut state = state.unwrap();

                    if state.last_updated_epoch == EPOCH_UNDEFINED {
                        let deleted = msm
                            .pending_deals
                            .as_mut()
                            .unwrap()
                            .delete(&dcid.to_bytes())
                            .map_err(|e| {
                                actor_error!(ErrIllegalState;
                                "failed to delete pending proposal: {}", e)
                            })?;
                        if !deleted {
                            return Err(actor_error!(ErrIllegalState;
                                    "failed to delete pending proposal: does not exist"));
                        }
                    }

                    let (slash_amount, next_epoch, remove_deal) =
                        msm.update_pending_deal_state(state, deal, curr_epoch)?;
                    assert_ne!(
                        slash_amount.sign(),
                        Sign::Minus,
                        "next scheduled epoch should be undefined as deal has been removed"
                    );

                    if remove_deal {
                        assert_eq!(
                            next_epoch, EPOCH_UNDEFINED,
                            "next scheduled epoch should be undefined as deal has been removed"
                        );

                        amount_slashed += slash_amount;
                        let deleted = msm
                            .deal_proposals
                            .as_mut()
                            .unwrap()
                            .delete(deal_id)
                            .map_err(|e| {
                                actor_error!(ErrIllegalState;
                                "failed to delete deal proposal: {}", e)
                            })?;
                        if !deleted {
                            return Err(actor_error!(ErrIllegalState;
                                "failed to delete deal proposal: does not exist"));
                        }

                        let deleted = msm.deal_states.as_mut().unwrap().delete(deal_id).map_err(
                            |e| actor_error!(ErrIllegalState; "failed to delete deal state: {}", e),
                        )?;
                        if !deleted {
                            return Err(actor_error!(ErrIllegalState;
                                    "failed to delete deal state: does not exist"));
                        }
                    } else {
                        assert!(
                            next_epoch > curr_epoch && slash_amount.is_zero(),
                            "deal should not be slashed and should have a schedule for next cron"
                        );

                        state.last_updated_epoch = curr_epoch;
                        msm.deal_states
                            .as_mut()
                            .unwrap()
                            .set(deal_id, state)
                            .map_err(|e| {
                                actor_error!(ErrIllegalState;
                                    "failed to set deal state: {}", e)
                            })?;

                        if let Some(ev) = updates_needed.get_mut(&next_epoch) {
                            ev.push(deal_id);
                        } else {
                            updates_needed.insert(next_epoch, vec![deal_id]);
                        }
                    }
                }
                msm.deals_by_epoch
                    .as_mut()
                    .unwrap()
                    .remove_all(i)
                    .map_err(|e| {
                        actor_error!(ErrIllegalState;
                            "failed to delete deal ops for epoch {}: {}", i, e)
                    })?;
            }

            // Iterate changes in sorted order to ensure that loads/stores
            // are deterministic. Otherwise, we could end up charging an
            // inconsistent amount of gas.
            let mut changed_epochs: Vec<ChainEpoch> = updates_needed.keys().cloned().collect();
            changed_epochs.sort_unstable();

            for epoch in changed_epochs {
                msm.deals_by_epoch
                    .as_mut()
                    .unwrap()
                    .put_many(
                        epoch,
                        updates_needed.get(&epoch).expect("key checked to exist"),
                    )
                    .map_err(|e| {
                        actor_error!(ErrIllegalState;
                            "failed to reinsert deal IDs for epoch {}: {}", epoch, e)
                    })?;
            }

            msm.st.last_cron = rt.curr_epoch();

            msm.commit_state()
                .map_err(|e| actor_error!(ErrIllegalState; "failed to flush state: {}", e))?;
            Ok(())
        })??;

        for d in timed_out_verified_deals {
            let res = rt.send(
                *VERIFIED_REGISTRY_ACTOR_ADDR,
                VerifregMethod::RestoreBytes as u64,
                Serialized::serialize(RestoreBytesParams {
                    address: d.client,
                    deal_size: BigInt::from(d.piece_size.0),
                })?,
                TokenAmount::zero(),
            );
            if let Err(e) = res {
                log::error!(
                    "failed to send RestoreBytes call to the verifreg actor for timed \
                    out verified deal, client: {}, deal_size: {}, provider: {}, got code: {:?}. {}",
                    d.client,
                    d.piece_size.0,
                    d.provider,
                    e.exit_code(),
                    e.msg()
                );
            }
        }

        if !amount_slashed.is_zero() {
            rt.send(
                *BURNT_FUNDS_ACTOR_ADDR,
                METHOD_SEND,
                Serialized::default(),
                amount_slashed,
            )?;
        }
        Ok(())
    }
}

/// Validates a collection of deal dealProposals for activation, and returns their combined weight,
/// split into regular deal weight and verified deal weight.
pub fn validate_deals_for_activation<BS>(
    st: &State,
    store: &BS,
    deal_ids: &[DealID],
    miner_addr: &Address,
    sector_expiry: ChainEpoch,
    curr_epoch: ChainEpoch,
) -> Result<(BigInt, BigInt), Box<dyn StdError>>
where
    BS: BlockStore,
{
    let proposals = DealArray::load(&st.proposals, store)?;

    let mut total_deal_space_time = BigInt::zero();
    let mut total_verified_space_time = BigInt::zero();
    for deal_id in deal_ids {
        let proposal = proposals
            .get(*deal_id)?
            .ok_or_else(|| actor_error!(ErrNotFound; "no such deal {}", deal_id))?;

        validate_deal_can_activate(&proposal, miner_addr, sector_expiry, curr_epoch)
            .map_err(|e| e.wrap(&format!("cannot activate deal {}", deal_id)))?;

        let deal_space_time = deal_weight(&proposal);
        if proposal.verified_deal {
            total_verified_space_time += deal_space_time;
        } else {
            total_deal_space_time += deal_space_time;
        }
    }

    Ok((total_deal_space_time, total_verified_space_time))
}

fn gen_rand_next_epoch<BS, RT>(
    rt: &RT,
    curr_epoch: ChainEpoch,
    deal: &DealProposal,
) -> Result<ChainEpoch, Box<dyn StdError>>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let bytes = deal
        .marshal_cbor()
        .map_err(|e| format!("failed to marshal proposal: {}", e))?;

    let rb = rt.get_randomness_from_beacon(
        DomainSeparationTag::MarketDealCronSeed,
        curr_epoch - 1,
        &bytes,
    )?;

    // generate a random epoch in [baseEpoch, baseEpoch + DealUpdatesInterval)
    let offset = BigEndian::read_u64(&rb.0);

    Ok(deal.start_epoch + (offset % DEAL_UPDATES_INTERVAL as u64) as ChainEpoch)
}
////////////////////////////////////////////////////////////////////////////////
// Checks
////////////////////////////////////////////////////////////////////////////////
fn validate_deal_can_activate(
    proposal: &DealProposal,
    miner_addr: &Address,
    sector_expiration: ChainEpoch,
    curr_epoch: ChainEpoch,
) -> Result<(), ActorError> {
    if &proposal.provider != miner_addr {
        return Err(actor_error!(ErrForbidden;
                "proposal has provider {}, must be {}", proposal.provider, miner_addr));
    };

    if curr_epoch > proposal.start_epoch {
        return Err(actor_error!(ErrIllegalArgument;
                "proposal start epoch {} has already elapsed at {}",
                proposal.start_epoch, curr_epoch));
    };

    if proposal.end_epoch > sector_expiration {
        return Err(actor_error!(ErrIllegalArgument;
                "proposal expiration {} exceeds sector expiration {}",
                proposal.end_epoch, sector_expiration));
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
        let (owner_addr, worker_addr, _) = request_miner_control_addrs(rt, nominal)?;
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
            Some(Method::VerifyDealsForActivation) => {
                let res = Self::verify_deals_for_activation(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::ActivateDeals) => {
                Self::activate_deals(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::OnMinerSectorsTerminate) => {
                Self::on_miner_sectors_terminate(rt, params.deserialize()?)?;
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

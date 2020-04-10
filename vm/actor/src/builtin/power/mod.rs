// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod policy;
mod state;
mod types;

pub use self::policy::*;
pub use self::state::{Claim, CronEvent, State};
pub use self::types::*;
use crate::{
    check_empty_params, init, request_miner_control_addrs, BalanceTable, StoragePower,
    BURNT_FUNDS_ACTOR_ADDR, CALLER_TYPES_SIGNABLE, CRON_ACTOR_ADDR, HAMT_BIT_WIDTH,
    INIT_ACTOR_ADDR, MINER_ACTOR_CODE_ID,
};
use address::Address;
use ipld_blockstore::BlockStore;
use ipld_hamt::Hamt;
use message::Message;
use num_bigint::biguint_ser::BigUintSer;
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Zero};
use runtime::{ActorCode, Runtime};
use std::convert::TryFrom;
use vm::{
    ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR, METHOD_SEND,
};

/// Storage power actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    /// Constructor for Storage Power Actor
    Constructor = METHOD_CONSTRUCTOR,
    AddBalance = 2,
    WithdrawBalance = 3,
    CreateMiner = 4,
    DeleteMiner = 5,
    OnSectorProveCommit = 6,
    OnSectorTerminate = 7,
    OnSectorTemporaryFaultEffectiveBegin = 8,
    OnSectorTemporaryFaultEffectiveEnd = 9,
    OnSectorModifyWeightDesc = 10,
    OnMinerWindowedPoStSuccess = 11,
    OnMinerWindowedPoStFailure = 12,
    EnrollCronEvent = 13,
    ReportConsensusFault = 14,
    OnEpochTickEnd = 15,
}

impl Method {
    /// Converts a method number into an Method enum
    fn from_method_num(m: MethodNum) -> Option<Method> {
        FromPrimitive::from_u64(m)
    }
}

/// Storage Power Actor
pub struct Actor;
impl Actor {
    /// Constructor for StoragePower actor
    pub fn constructor<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let empty_root = Hamt::<String, _>::new_with_bit_width(rt.store(), HAMT_BIT_WIDTH)
            .flush()
            .map_err(|e| {
                rt.abort(
                    ExitCode::ErrIllegalState,
                    format!("failed to create storage power state: {}", e),
                )
            })?;
        let st = State::new(empty_root);
        rt.create(&st)?;
        Ok(())
    }
    pub fn add_balance<BS, RT>(rt: &mut RT, params: AddBalanceParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let nominal = rt.resolve_address(&params.miner)?;

        validate_pledge_account(rt, &nominal)?;
        let (owner_addr, worker_addr) = request_miner_control_addrs(rt, &nominal)?;
        rt.validate_immediate_caller_is(&[owner_addr, worker_addr])?;

        let msg = rt.message().value().clone();
        rt.transaction(|st: &mut State, rt| {
            st.add_miner_balance(rt.store(), &nominal, &msg)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to add pledge balance: {}", e),
                    )
                })?;
            Ok(())
        })?
    }
    pub fn withdraw_balance<BS, RT>(
        rt: &mut RT,
        params: WithdrawBalanceParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let nominal = rt.resolve_address(&params.miner)?;

        validate_pledge_account(rt, &nominal)?;
        let (owner_addr, worker_addr) = request_miner_control_addrs(rt, &nominal)?;
        rt.validate_immediate_caller_is(&[owner_addr.clone(), worker_addr])?;

        if params.requested < TokenAmount::zero() {
            return Err(rt.abort(
                ExitCode::ErrIllegalArgument,
                format!("negative withdrawal {}", params.requested),
            ));
        }

        let amount_extracted =
            rt.transaction::<State, Result<TokenAmount, ActorError>, _>(|st, rt| {
                let claim = Self::get_claim_or_abort(st, rt.store(), &nominal)?;

                Ok(st
                    .subtract_miner_balance(rt.store(), &nominal, &params.requested, &claim.pledge)
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("failed to subtract pledge balance: {}", e),
                        )
                    })?)
            })??;

        rt.send(
            &owner_addr,
            METHOD_SEND,
            &Serialized::default(),
            &amount_extracted,
        )?;
        Ok(())
    }
    pub fn create_miner<BS, RT>(
        rt: &mut RT,
        params: &Serialized,
    ) -> Result<CreateMinerReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;

        let addresses: init::ExecReturn = rt
            .send(
                &INIT_ACTOR_ADDR,
                init::Method::Exec as u64,
                params,
                &TokenAmount::from(0u8),
            )?
            .deserialize()?;

        let value = rt.message().value().clone();
        rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
            st.set_miner_balance(rt.store(), &addresses.id_address, value)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to set pledge balance {}", e),
                    )
                })?;

            st.set_claim(rt.store(), &addresses.id_address, Claim::default())
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!(
                            "failed to put power in claimed table while creating miner: {}",
                            e
                        ),
                    )
                })?;
            st.miner_count += 1;
            Ok(())
        })??;
        Ok(CreateMinerReturn {
            id_address: addresses.id_address,
            robust_address: addresses.robust_address,
        })
    }
    pub fn delete_miner<BS, RT>(rt: &mut RT, params: DeleteMinerParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let nominal = rt.resolve_address(&params.miner)?;

        let st: State = rt.state()?;

        let balance = st.get_miner_balance(rt.store(), &nominal).map_err(|e| {
            rt.abort(
                ExitCode::ErrIllegalState,
                format!("failed to get pledge balance for deletion: {}", e),
            )
        })?;
        if balance > TokenAmount::zero() {
            return Err(rt.abort(
                ExitCode::ErrForbidden,
                format!(
                    "deletion requested for miner {} with pledge balance {}",
                    nominal, balance
                ),
            ));
        }

        let claim = Self::get_claim_or_abort(&st, rt.store(), &nominal)?;

        if claim.power > Zero::zero() {
            return Err(rt.abort(
                ExitCode::ErrIllegalState,
                format!(
                    "deletion requested for miner {} with power {}",
                    nominal, claim.power
                ),
            ));
        }

        let (owner_addr, worker_addr) = request_miner_control_addrs(rt, &nominal)?;
        rt.validate_immediate_caller_is(&[owner_addr, worker_addr])?;

        Self::delete_miner_actor(rt, &nominal)
    }
    pub fn on_sector_prove_commit<BS, RT>(
        rt: &mut RT,
        params: OnSectorProveCommitParams,
    ) -> Result<TokenAmount, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;

        let from = rt.message().from().clone();
        rt.transaction(|st: &mut State, rt| {
            let power = consensus_power_for_weight(&params.weight);
            let pledge = pledge_for_weight(&params.weight, &st.total_network_power);
            st.add_to_claim(rt.store(), &from, &power, &pledge)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("Failed to add power for sector: {}", e),
                    )
                })?;
            Ok(pledge_for_weight(&params.weight, &st.total_network_power))
        })?
    }
    pub fn on_sector_terminate<BS, RT>(
        rt: &mut RT,
        params: OnSectorTerminateParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        let miner_addr = rt.message().from().clone();

        rt.transaction(|st: &mut State, rt| {
            let power = consensus_power_for_weights(&params.weights);
            st.subtract_from_claim(rt.store(), &miner_addr, &power, &params.pledge)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to deduct claimed power for sector: {}", e),
                    )
                })
        })??;

        if params.termination_type != SECTOR_TERMINATION_EXPIRED {
            let amount_to_slash = pledge_penalty_for_sector_termination(
                &TokenAmount::try_from(params.pledge).unwrap(),
                params.termination_type,
            );
            Self::slash_pledge_collateral(rt, &miner_addr, amount_to_slash)?;
        }

        Ok(())
    }
    pub fn on_sector_temporary_fault_effective_begin<BS, RT>(
        rt: &mut RT,
        params: OnSectorTemporaryFaultEffectiveBeginParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;

        let from = rt.message().from().clone();
        rt.transaction(|st: &mut State, rt| {
            let power = consensus_power_for_weights(&params.weights);
            st.subtract_from_claim(rt.store(), &from, &power, &params.pledge)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to deduct claimed power for sector: {}", e),
                    )
                })
        })?
    }
    pub fn on_sector_temporary_fault_effective_end<BS, RT>(
        rt: &mut RT,
        params: OnSectorTemporaryFaultEffectiveEndParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;

        let from = rt.message().from().clone();
        rt.transaction(|st: &mut State, rt| {
            let power = consensus_power_for_weights(&params.weights);
            st.add_to_claim(rt.store(), &from, &power, &params.pledge)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to add claimed power for sector: {}", e),
                    )
                })
        })?
    }
    pub fn on_sector_modify_weight_desc<BS, RT>(
        rt: &mut RT,
        params: OnSectorModifyWeightDescParams,
    ) -> Result<TokenAmount, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        let from = rt.message().from().clone();
        let msg = rt.message().from().clone();
        rt.transaction(|st: &mut State, rt| {
            let prev_power = consensus_power_for_weight(&params.prev_weight);
            st.subtract_from_claim(rt.store(), &msg, &prev_power, &params.prev_pledge)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to deduct claimed power for sector: {}", e),
                    )
                })?;
            let new_power = consensus_power_for_weight(&params.new_weight);
            let new_pledge = pledge_for_weight(&params.new_weight, &st.total_network_power);
            st.add_to_claim(rt.store(), &from, &new_power, &new_pledge)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to deduct claimed power for sector: {}", e),
                    )
                })?;
            Ok(new_pledge)
        })?
    }
    pub fn on_miner_windowed_post_success<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        let miner_addr = rt.message().from().clone();
        rt.transaction(
            |st: &mut State, rt| match st.has_detected_fault(rt.store(), &miner_addr) {
                Ok(false) => Ok(()),
                Ok(true) => st
                    .delete_detected_fault(rt.store(), &miner_addr)
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("failed to check miner for detected fault: {}", e),
                        )
                    }),
                Err(e) => Err(ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to check miner for detected fault: {}", e),
                )),
            },
        )?
    }
    pub fn on_miner_windowed_post_failure<BS, RT>(
        rt: &mut RT,
        params: OnMinerWindowedPoStFailureParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        let miner_addr = rt.message().from().clone();
        let claim: Option<Claim> =
            rt.transaction::<_, Result<_, ActorError>, _>(|st: &mut State, rt| {
                let faulty = st
                    .has_detected_fault(rt.store(), &miner_addr)
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("failed to check if miner was faulty already: {}", e),
                        )
                    })?;
                if faulty {
                    return Ok(None);
                }
                st.put_detected_fault(rt.store(), &miner_addr)
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("failed to put miner fault: {}", e),
                        )
                    })?;

                let claim = Self::get_claim_or_abort(st, rt.store(), &miner_addr)?;

                if claim.power >= *CONSENSUS_MINER_MIN_POWER {
                    st.total_network_power -= &claim.power;
                }

                Ok(Some(claim))
            })??;
        let claim = match claim {
            Some(cl) => cl,
            None => return Ok(()),
        };

        if params.num_consecutive_failures > WINDOWED_POST_FAILURE_LIMIT {
            Self::delete_miner_actor(rt, &miner_addr)?;
        } else {
            let amount_to_slash = pledge_penalty_for_windowed_post_failure(
                &claim.pledge,
                params.num_consecutive_failures,
            );
            Self::slash_pledge_collateral(rt, &miner_addr, amount_to_slash)?;
        }
        Ok(())
    }
    pub fn enroll_cron_event<BS, RT>(
        rt: &mut RT,
        params: EnrollCronEventParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        let miner_addr = rt.message().from().clone();
        let miner_event = CronEvent {
            miner_addr,
            callback_payload: params.payload.clone(),
        };

        rt.transaction(|st: &mut State, rt| {
            st.append_cron_event(rt.store(), params.event_epoch, miner_event)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to enroll cron event: {}", e),
                    )
                })
        })?
    }
    pub fn report_consensus_fault<BS, RT>(
        rt: &mut RT,
        params: ReportConsensusFaultParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let curr_epoch = rt.curr_epoch();
        let earliest = curr_epoch - CONSENSUS_FAULT_REPORTING_WINDOW;
        let fault = rt
            .syscalls()
            .verify_consensus_fault(
                params.block_header_1.bytes(),
                params.block_header_2.bytes(),
                params.block_header_extra.bytes(),
                earliest,
            )
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("fault not verified: {}", e),
                )
            })?;

        let reporter = rt.message().from().clone();
        let reward = rt.transaction(|st: &mut State, rt| {
            let claim = Self::get_claim_or_abort(st, rt.store(), &fault.target)?;
            let curr_balance = st
                .get_miner_balance(rt.store(), &fault.target)
                .map_err(|_| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        "failed to get miner pledge balance".to_owned(),
                    )
                })?;
            assert!(claim.power >= StoragePower::from(0));

            // Elapsed since the fault (i.e. since the higher of the two blocks)
            let fault_age = curr_epoch.checked_sub(fault.epoch).ok_or_else(|| {
                ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!(
                        "invalid fault epoch {:?} ahead of current {:?}",
                        fault.epoch, curr_epoch
                    ),
                )
            })?;
            // Note: this slashes the miner's whole balance, including any excess over the required claim.Pledge.
            let collateral_to_slash =
                pledge_penalty_for_consensus_fault(curr_balance, fault.fault_type);
            let target_reward = reward_for_consensus_slash_report(fault_age, collateral_to_slash);

            st.subtract_miner_balance(
                rt.store(),
                &fault.target,
                &target_reward,
                &TokenAmount::from(0u8),
            )
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to subtract pledge for reward: {}", e),
                )
            })
        })??;

        rt.send(&reporter, METHOD_SEND, &Serialized::default(), &reward)?;

        Self::delete_miner_actor(rt, &fault.target)
    }
    pub fn on_epoch_tick_end<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*CRON_ACTOR_ADDR))?;

        let rt_epoch = rt.curr_epoch();
        let cron_events = rt
            .transaction::<_, Result<_, String>, _>(|st: &mut State, rt| {
                let mut events = Vec::new();
                for i in st.last_epoch_tick..=rt_epoch {
                    // Load epoch cron events
                    let epoch_events = st.load_cron_events(rt.store(), i)?;

                    // Add all events to vector
                    events.extend_from_slice(&epoch_events);

                    // Clear loaded events
                    if !epoch_events.is_empty() {
                        st.clear_cron_events(rt.store(), i)?;
                    }
                }
                st.last_epoch_tick = rt_epoch;
                Ok(events)
            })?
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("Failed to clear cron events: {}", e),
                )
            })?;

        for event in cron_events {
            // TODO switch 12 to OnDeferredCronEvent on miner actor impl
            rt.send(
                &event.miner_addr,
                12,
                &event.callback_payload,
                &TokenAmount::from(0u8),
            )?;
        }

        Ok(())
    }

    fn delete_miner_actor<BS, RT>(rt: &mut RT, miner: &Address) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let amount_slashed: TokenAmount = rt
            .transaction::<_, Result<_, String>, _>(|st: &mut State, rt| {
                st.delete_claim(rt.store(), miner)?;

                st.miner_count -= 1;

                if st.has_detected_fault(rt.store(), miner)? {
                    st.delete_detected_fault(rt.store(), miner)?;
                }

                let mut table = BalanceTable::from_root(rt.store(), &st.escrow_table)?;
                let balance = table.remove(miner)?;

                st.escrow_table = table.root()?;

                Ok(balance)
            })?
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e))?;

        // TODO switch 6 to OnDeleteMiner on miner actor impl
        rt.send(
            &miner,
            6,
            &Serialized::serialize(&*BURNT_FUNDS_ACTOR_ADDR)?,
            &TokenAmount::from(0u8),
        )?;
        rt.send(
            &*BURNT_FUNDS_ACTOR_ADDR,
            METHOD_SEND,
            &Serialized::default(),
            &amount_slashed,
        )?;

        Ok(())
    }

    fn slash_pledge_collateral<BS, RT>(
        rt: &mut RT,
        miner_addr: &Address,
        amount_to_slash: TokenAmount,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let amount_slashed = rt.transaction(|st: &mut State, rt| {
            st.subtract_miner_balance(
                rt.store(),
                miner_addr,
                &amount_to_slash,
                &TokenAmount::from(0u8),
            )
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to subtract collateral for slash: {}", e),
                )
            })
        })??;

        rt.send(
            &*BURNT_FUNDS_ACTOR_ADDR,
            METHOD_SEND,
            &Serialized::default(),
            &amount_slashed,
        )?;

        Ok(())
    }

    fn get_claim_or_abort<BS: BlockStore>(
        st: &State,
        store: &BS,
        a: &Address,
    ) -> Result<Claim, ActorError> {
        st.get_claim(store, a)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to load claim for miner {}: {}", a, e),
                )
            })?
            .ok_or_else(|| {
                ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("no claim for miner {}", a),
                )
            })
    }
}

fn validate_pledge_account<BS, RT>(rt: &RT, addr: &Address) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let code_id = rt.get_actor_code_cid(addr)?;
    if code_id != *MINER_ACTOR_CODE_ID {
        Err(ActorError::new(
            ExitCode::ErrIllegalArgument,
            format!(
                "pledge account {} must be address of miner actor, was {}",
                addr, code_id
            ),
        ))
    } else {
        Ok(())
    }
}

fn consensus_power_for_weights(weights: &[SectorStorageWeightDesc]) -> StoragePower {
    let mut power = StoragePower::zero();
    for weight in weights {
        power += consensus_power_for_weight(weight);
    }
    power
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
        match Method::from_method_num(method) {
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
            Some(Method::CreateMiner) => {
                let res = Self::create_miner(rt, params)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::DeleteMiner) => {
                Self::delete_miner(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::OnSectorProveCommit) => {
                let res = Self::on_sector_prove_commit(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(BigUintSer(&res))?)
            }
            Some(Method::OnSectorTerminate) => {
                Self::on_sector_terminate(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::OnSectorTemporaryFaultEffectiveBegin) => {
                Self::on_sector_temporary_fault_effective_begin(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::OnSectorTemporaryFaultEffectiveEnd) => {
                Self::on_sector_temporary_fault_effective_end(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::OnSectorModifyWeightDesc) => {
                let res = Self::on_sector_modify_weight_desc(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(BigUintSer(&res))?)
            }
            Some(Method::OnMinerWindowedPoStSuccess) => {
                check_empty_params(params)?;
                Self::on_miner_windowed_post_success(rt)?;
                Ok(Serialized::default())
            }
            Some(Method::OnMinerWindowedPoStFailure) => {
                Self::on_miner_windowed_post_failure(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::EnrollCronEvent) => {
                Self::enroll_cron_event(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::ReportConsensusFault) => {
                Self::report_consensus_fault(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::OnEpochTickEnd) => {
                check_empty_params(params)?;
                Self::on_epoch_tick_end(rt)?;
                Ok(Serialized::default())
            }
            _ => Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method")),
        }
    }
}

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod policy;
mod state;
mod types;

pub use self::policy::*;
pub use self::state::{Claim, CronEvent, State};
pub use self::types::*;
use crate::{
    check_empty_params, init, request_miner_control_addrs, StoragePower, BURNT_FUNDS_ACTOR_ADDR,
    CALLER_TYPES_SIGNABLE, HAMT_BIT_WIDTH, INIT_ACTOR_ADDR, MINER_ACTOR_CODE_ID,
};
use address::Address;
use ipld_blockstore::BlockStore;
use ipld_hamt::Hamt;
use message::Message;
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Zero};
use runtime::{ActorCode, Runtime};
use std::convert::TryFrom;
use vm::{
    ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR, METHOD_SEND,
};

/// Storage power actor methods available
#[derive(FromPrimitive)]
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
        FromPrimitive::from_u64(u64::from(m))
    }
}

/// Storage Power Actor
pub struct Actor;
impl Actor {
    /// Constructor for StoragePower actor
    pub fn constructor<BS, RT>(rt: &RT) -> Result<(), ActorError>
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
        rt.create(&st);
        Ok(())
    }
    pub fn add_balance<BS, RT>(rt: &RT, params: AddBalanceParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let nominal = rt.resolve_address(&params.miner).ok_or(rt.abort(
            ExitCode::ErrIllegalArgument,
            format!("failed to resolve address {}", params.miner),
        ))?;

        validate_pledge_account(rt, &nominal)?;
        let (owner_addr, worker_addr) = request_miner_control_addrs(rt, &nominal)?;
        rt.validate_immediate_caller_is([owner_addr.clone(), worker_addr.clone()].iter());

        rt.transaction(|st: &mut State| {
            st.add_miner_balance(rt.store(), &nominal, rt.message().value())
                .map_err(|e| {
                    rt.abort(
                        ExitCode::ErrIllegalState,
                        format!("failed to add pledge balance: {}", e),
                    )
                })?;
            Ok(())
        })
    }
    pub fn withdraw_balance<BS, RT>(
        rt: &RT,
        params: WithdrawBalanceParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let nominal = rt.resolve_address(&params.miner).ok_or(rt.abort(
            ExitCode::ErrIllegalArgument,
            format!("failed to resolve address {}", params.miner),
        ))?;

        validate_pledge_account(rt, &nominal)?;
        let (owner_addr, worker_addr) = request_miner_control_addrs(rt, &nominal)?;
        rt.validate_immediate_caller_is([owner_addr.clone(), worker_addr.clone()].iter());

        if params.requested < TokenAmount::new(0) {
            return Err(rt.abort(
                ExitCode::ErrIllegalArgument,
                format!("negative withdrawal {}", params.requested),
            ));
        }

        let amount_extracted =
            rt.transaction::<State, Result<TokenAmount, ActorError>, _>(|st| {
                let claim = Self::get_claim_or_abort(st, rt.store(), &nominal)?;

                Ok(st
                    .subtract_miner_balance(rt.store(), &nominal, &params.requested, &claim.pledge)
                    .map_err(|e| {
                        rt.abort(
                            ExitCode::ErrIllegalState,
                            format!("failed to subtract pledge balance: {}", e),
                        )
                    })?)
            })?;

        rt.send(
            &owner_addr,
            MethodNum(METHOD_SEND as u64),
            &Serialized::default(),
            &amount_extracted,
        )?;
        Ok(())
    }
    pub fn create_miner<BS, RT>(
        rt: &RT,
        params: &Serialized,
    ) -> Result<CreateMinerReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter());

        let addresses: init::ExecReturn = rt
            .send(
                &INIT_ACTOR_ADDR,
                MethodNum(init::Method::Exec as u64),
                params,
                &TokenAmount::new(0),
            )?
            .deserialize()?;

        rt.transaction::<State, Result<(), ActorError>, _>(|st| {
            st.set_miner_balance(
                rt.store(),
                &addresses.id_address,
                rt.message().value().clone(),
            )
            .map_err(|e| {
                rt.abort(
                    ExitCode::ErrIllegalState,
                    format!("failed to set pledge balance {}", e),
                )
            })?;

            st.set_claim(rt.store(), &addresses.id_address, Claim::default())
                .map_err(|e| {
                    rt.abort(
                        ExitCode::ErrIllegalState,
                        format!(
                            "failed to put power in claimed table while creating miner: {}",
                            e
                        ),
                    )
                })?;
            st.miner_count += 1;
            Ok(())
        })?;
        Ok(CreateMinerReturn {
            id_address: addresses.id_address,
            robust_address: addresses.robust_address,
        })
    }
    pub fn delete_miner<BS, RT>(rt: &RT, params: DeleteMinerParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let nominal = rt.resolve_address(&params.miner).ok_or(rt.abort(
            ExitCode::ErrIllegalArgument,
            format!("failed to resolve address {}", params.miner),
        ))?;

        let st: State = rt.state();

        let balance = st.get_miner_balance(rt.store(), &nominal).map_err(|e| {
            rt.abort(
                ExitCode::ErrIllegalState,
                format!("failed to get pledge balance for deletion: {}", e),
            )
        })?;
        if balance > TokenAmount::new(0) {
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
        rt.validate_immediate_caller_is([owner_addr, worker_addr].iter());

        Self::delete_miner_actor(rt, &nominal)
    }
    pub fn on_sector_prove_commit<BS, RT>(
        rt: &RT,
        params: OnSectorProveCommitParams,
    ) -> Result<TokenAmount, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID));

        rt.transaction(|st: &mut State| {
            let power = consensus_power_for_weight(&params.weight);
            let pledge = pledge_for_weight(&params.weight, &st.total_network_power);
            st.add_to_claim(rt.store(), rt.message().from(), &power, &pledge.into())
                .map_err(|e| {
                    rt.abort(
                        ExitCode::ErrIllegalState,
                        format!("Failed to add power for sector: {}", e),
                    )
                })?;
            Ok(pledge_for_weight(&params.weight, &st.total_network_power))
        })
    }
    pub fn on_sector_terminate<BS, RT>(
        rt: &RT,
        params: OnSectorTerminateParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID));
        let miner_addr = rt.message().from().clone();

        rt.transaction(|st: &mut State| {
            let power = consensus_power_for_weights(&params.weights);
            st.subtract_from_claim(rt.store(), &miner_addr, &power, &params.pledge)
                .map_err(|e| {
                    rt.abort(
                        ExitCode::ErrIllegalState,
                        format!("failed to deduct claimed power for sector: {}", e),
                    )
                })
        })?;

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
        rt: &RT,
        params: OnSectorTemporaryFaultEffectiveBeginParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID));

        rt.transaction(|st: &mut State| {
            let power = consensus_power_for_weights(&params.weights);
            st.subtract_from_claim(
                rt.store(),
                &rt.message().from().clone(),
                &power,
                &params.pledge,
            )
            .map_err(|e| {
                rt.abort(
                    ExitCode::ErrIllegalState,
                    format!("failed to deduct claimed power for sector: {}", e),
                )
            })
        })
    }
    pub fn on_sector_temporary_fault_effective_end<BS, RT>(
        rt: &RT,
        params: OnSectorTemporaryFaultEffectiveEndParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID));

        rt.transaction(|st: &mut State| {
            let power = consensus_power_for_weights(&params.weights);
            st.add_to_claim(
                rt.store(),
                &rt.message().from().clone(),
                &power,
                &params.pledge,
            )
            .map_err(|e| {
                rt.abort(
                    ExitCode::ErrIllegalState,
                    format!("failed to add claimed power for sector: {}", e),
                )
            })
        })
    }
    pub fn on_sector_modify_weight_desc<BS, RT>(
        rt: &RT,
        params: OnSectorModifyWeightDescParams,
    ) -> Result<TokenAmount, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID));

        rt.transaction(|st: &mut State| {
            let prev_power = consensus_power_for_weight(&params.prev_weight);
            st.subtract_from_claim(
                rt.store(),
                rt.message().from(),
                &prev_power,
                &params.prev_pledge,
            )
            .map_err(|e| {
                rt.abort(
                    ExitCode::ErrIllegalState,
                    format!("failed to deduct claimed power for sector: {}", e),
                )
            })?;
            let new_power = consensus_power_for_weight(&params.new_weight);
            let new_pledge = pledge_for_weight(&params.new_weight, &st.total_network_power);
            st.add_to_claim(rt.store(), rt.message().from(), &new_power, &new_pledge)
                .map_err(|e| {
                    rt.abort(
                        ExitCode::ErrIllegalState,
                        format!("failed to deduct claimed power for sector: {}", e),
                    )
                })?;
            Ok(new_pledge)
        })
    }
    pub fn on_miner_windowed_post_success<BS, RT>(rt: &RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID));
        let miner_addr = rt.message().from().clone();
        rt.transaction(
            |st: &mut State| match st.has_detected_fault(rt.store(), &miner_addr) {
                Ok(false) => Ok(()),
                Ok(true) => st
                    .delete_detected_fault(rt.store(), &miner_addr)
                    .map_err(|e| {
                        rt.abort(
                            ExitCode::ErrIllegalState,
                            format!("failed to check miner for detected fault: {}", e),
                        )
                    }),
                Err(e) => Err(rt.abort(
                    ExitCode::ErrIllegalState,
                    format!("failed to check miner for detected fault: {}", e),
                )),
            },
        )
    }
    pub fn on_miner_windowed_post_failure<BS, RT>(
        rt: &RT,
        params: OnMinerWindowedPoStFailureParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID));
        let miner_addr = rt.message().from().clone();
        let claim: Option<Claim> =
            rt.transaction::<_, Result<_, ActorError>, _>(|st: &mut State| {
                let faulty = st
                    .has_detected_fault(rt.store(), &miner_addr)
                    .map_err(|e| {
                        rt.abort(
                            ExitCode::ErrIllegalState,
                            format!("failed to check if miner was faulty already: {}", e),
                        )
                    })?;
                if faulty {
                    return Ok(None);
                }
                st.put_detected_fault(rt.store(), &miner_addr)
                    .map_err(|e| {
                        rt.abort(
                            ExitCode::ErrIllegalState,
                            format!("failed to put miner fault: {}", e),
                        )
                    })?;

                let claim = Self::get_claim_or_abort(st, rt.store(), &miner_addr)?;

                if claim.power >= *CONSENSUS_MINER_MIN_POWER {
                    st.total_network_power -= &claim.power;
                }

                Ok(Some(claim))
            })?;
        let claim = match claim {
            Some(cl) => cl,
            None => return Ok(()),
        };

        if params.num_consecutive_failures > WINDOWED_POST_FAILURE_LIMIT {
            Self::delete_miner_actor(rt, &miner_addr)?;
        } else {
            let amount_to_slash = pledge_penalty_for_sector_termination(
                &claim.pledge,
                params.num_consecutive_failures,
            );
            Self::slash_pledge_collateral(rt, &miner_addr, amount_to_slash)?;
        }
        Ok(())
    }
    pub fn enroll_cron_event<BS, RT>(
        rt: &RT,
        params: EnrollCronEventParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID));
        let miner_addr = rt.message().from().clone();
        let miner_event = CronEvent {
            miner_addr,
            callback_payload: params.payload.clone(),
        };

        rt.transaction(|st: &mut State| {
            st.append_cron_event(rt.store(), params.event_epoch, &miner_event)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to enroll cron event: {}", e),
                    )
                })
        })
    }
    pub fn report_consensus_fault<BS, RT>(
        rt: &RT,
        params: ReportConsensusFaultParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let curr_epoch = rt.curr_epoch();
        let earliest = curr_epoch - CONSENSUS_FAULT_REPORTING_WINDOW;
        todo!()
    }
    pub fn on_epoch_tick_end<BS, RT>(rt: &RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!();
    }

    fn delete_miner_actor<BS, RT>(_rt: &RT, _miner: &Address) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        todo!()
    }

    fn slash_pledge_collateral<BS, RT>(
        rt: &RT,
        miner_addr: &Address,
        amount_to_slash: TokenAmount,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let amount_slashed = rt.transaction(|st: &mut State| {
            st.subtract_miner_balance(
                rt.store(),
                miner_addr,
                &amount_to_slash,
                &TokenAmount::new(0),
            )
            .map_err(|e| {
                rt.abort(
                    ExitCode::ErrIllegalState,
                    format!("failed to subtract collateral for slash: {}", e),
                )
            })
        })?;

        rt.send(
            &*BURNT_FUNDS_ACTOR_ADDR,
            MethodNum(METHOD_SEND as u64),
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
            .ok_or(ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!("no claim for miner {}", a),
            ))
    }
}

fn validate_pledge_account<BS, RT>(_rt: &RT, _addr: &Address) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    todo!()
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
        rt: &RT,
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
                Ok(Serialized::serialize(res)?)
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
                Ok(Serialized::serialize(res)?)
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

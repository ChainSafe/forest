// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;

pub use self::state::{LaneState, Merge, State};
pub use self::types::*;
use crate::{check_empty_params, ACCOUNT_ACTOR_CODE_ID, INIT_ACTOR_CODE_ID};
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use encoding::to_vec;
use forest_ipld::Ipld;
use ipld_blockstore::BlockStore;
use message::Message;
use num_bigint::BigInt;
use num_derive::FromPrimitive;
use num_traits::{CheckedSub, FromPrimitive, Zero};
use runtime::{ActorCode, Runtime};
use vm::{
    ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR, METHOD_SEND,
};

/// Payment Channel actor methods available
#[derive(FromPrimitive)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    UpdateChannelState = 2,
    Settle = 3,
    Collect = 4,
}

impl Method {
    /// Converts a method number into an Method enum
    fn from_method_num(m: MethodNum) -> Option<Method> {
        FromPrimitive::from_u64(u64::from(m))
    }
}

/// Payment Channel actor
pub struct Actor;
impl Actor {
    /// Constructor for Payment channel actor
    pub fn constructor<BS, RT>(rt: &RT, params: ConstructorParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // Only InitActor can create a payment channel actor. It creates the actor on
        // behalf of the payer/payee.
        rt.validate_immediate_caller_type(std::iter::once::<&Cid>(&INIT_ACTOR_CODE_ID));

        // Check both parties are capable of signing vouchers
        let to = Self::resolve_account(rt, &params.to)
            .map_err(|e| rt.abort(ExitCode::ErrIllegalArgument, e))?;

        let from = Self::resolve_account(rt, &params.from)
            .map_err(|e| rt.abort(ExitCode::ErrIllegalArgument, e))?;

        rt.create(&State::new(from, to));
        Ok(())
    }

    /// Resolves an address to a canonical ID address and requires it to address an account actor.
    /// The account actor constructor checks that the embedded address is associated with an appropriate key.
    /// An alternative (more expensive) would be to send a message to the actor to fetch its key.
    fn resolve_account<BS, RT>(rt: &RT, raw: &Address) -> Result<Address, String>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let resolved = rt
            .resolve_address(raw)
            .ok_or(format!("failed to resolve address {}", raw))?;

        let code_cid = rt
            .get_actor_code_cid(&resolved)
            .ok_or(format!("no code for address {}", resolved))?;

        let account_code_ref: &Cid = &ACCOUNT_ACTOR_CODE_ID;
        if &code_cid != account_code_ref {
            Err(format!(
                "actor {} must be an account ({}), was {}",
                raw, account_code_ref, code_cid
            ))
        } else {
            Ok(resolved)
        }
    }

    pub fn update_channel_state<BS, RT>(
        rt: &RT,
        params: UpdateChannelStateParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let st: State = rt.state();

        rt.validate_immediate_caller_is([st.from.clone(), st.to.clone()].iter());
        let signer = if rt.message().from() == &st.from {
            st.to
        } else {
            st.from
        };

        let mut sv = params.sv;

        // Pull signature from signed voucher
        let sig = sv
            .signature
            .take()
            .ok_or_else(|| rt.abort(ExitCode::ErrIllegalArgument, "voucher has no signature"))?;

        // Generate unsigned bytes
        let sv_bz = to_vec(&sv).map_err(|_| {
            rt.abort(
                ExitCode::ErrIllegalArgument,
                "failed to serialize SignedVoucher",
            )
        })?;

        // Validate signature
        rt.syscalls()
            .verify_signature(&sig, &signer, &sv_bz)
            .map_err(|e| {
                rt.abort(
                    ExitCode::ErrIllegalArgument,
                    format!("voucher signature invalid: {}", e),
                )
            })?;

        if rt.curr_epoch() < sv.time_lock_min {
            return Err(rt.abort(ExitCode::ErrIllegalArgument, "cannot use this voucher yet"));
        }

        if sv.time_lock_max != ChainEpoch(0) && rt.curr_epoch() > sv.time_lock_max {
            return Err(rt.abort(ExitCode::ErrIllegalArgument, "this voucher has expired"));
        }

        if !sv.secret_pre_image.is_empty() {
            let hashed_secret: &[u8] = &rt.syscalls().hash_blake2b(&params.secret);
            if hashed_secret != sv.secret_pre_image.as_slice() {
                return Err(rt.abort(ExitCode::ErrIllegalArgument, "incorrect secret"));
            }
        }

        if let Some(extra) = &sv.extra {
            rt.send::<Ipld>(
                &extra.actor,
                extra.method,
                &Serialized::serialize(PaymentVerifyParams {
                    extra: extra.data.clone(),
                    proof: params.proof,
                })?,
                &TokenAmount::new(0),
            )?;
        }

        rt.transaction(|st: &mut State| {
            // Find the voucher lane, create and insert it in sorted order if necessary.
            let (idx, exists) = find_lane(&st.lane_states, sv.lane);
            if !exists {
                if st.lane_states.len() >= LANE_LIMIT {
                    return Err(rt.abort(ExitCode::ErrIllegalArgument, "lane limit exceeded"));
                }
                let tmp_ls = LaneState {
                    id: sv.lane,
                    redeemed: BigInt::zero(),
                    nonce: 0,
                };
                st.lane_states.insert(idx, tmp_ls);
            };
            // let mut ls = st.lane_states[idx].clone();

            if st.lane_states[idx].nonce > sv.nonce {
                return Err(rt.abort(
                    ExitCode::ErrIllegalArgument,
                    "voucher has an outdated nonce, cannot redeem",
                ));
            }

            // The next section actually calculates the payment amounts to update the payment channel state
            // 1. (optional) sum already redeemed value of all merging lanes
            let mut redeemed = BigInt::default();
            for merge in sv.merges {
                if merge.lane == sv.lane {
                    return Err(rt.abort(
                        ExitCode::ErrIllegalArgument,
                        "voucher cannot merge lanes into it's own lane",
                    ));
                }
                let (idx, exists) = find_lane(&st.lane_states, merge.lane);
                if exists {
                    if st.lane_states[idx].nonce >= merge.nonce {
                        return Err(rt.abort(
                            ExitCode::ErrIllegalArgument,
                            "merged lane in voucher has outdated nonce, cannot redeem",
                        ));
                    }

                    redeemed += &st.lane_states[idx].redeemed;
                    st.lane_states[idx].nonce = merge.nonce;
                } else {
                    return Err(rt.abort(
                        ExitCode::ErrIllegalArgument,
                        format!("voucher specifies invalid merge lane {}", merge.lane),
                    ));
                }
            }

            // 2. To prevent double counting, remove already redeemed amounts (from
            // voucher or other lanes) from the voucher amount
            st.lane_states[idx].nonce = sv.nonce;
            let balance_delta = &sv.amount - redeemed + &st.lane_states[idx].redeemed;

            // 3. set new redeemed value for merged-into lane
            st.lane_states[idx].redeemed = sv.amount;

            // 4. check operation validity
            let new_send_balance = st.to_send.add_bigint(balance_delta).map_err(|_| {
                rt.abort(
                    ExitCode::ErrIllegalState,
                    "voucher would leave channel balance negative",
                )
            })?;

            if new_send_balance > rt.current_balance() {
                return Err(rt.abort(
                    ExitCode::ErrIllegalState,
                    "not enough funds in channel to cover voucher",
                ));
            }

            // 5. add new redemption ToSend
            st.to_send = new_send_balance;

            // update channel settlingAt and MinSettleHeight if delayed by voucher
            if sv.min_settle_height != ChainEpoch(0) {
                if st.settling_at != ChainEpoch(0) && st.settling_at < sv.min_settle_height {
                    st.settling_at = sv.min_settle_height;
                }
                if st.min_settle_height < sv.min_settle_height {
                    st.min_settle_height = sv.min_settle_height;
                }
            }
            Ok(())
        })
    }

    pub fn settle<BS, RT>(rt: &RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.transaction(|st: &mut State| {
            rt.validate_immediate_caller_is([st.from.clone(), st.to.clone()].iter());

            if st.settling_at != ChainEpoch(0) {
                return Err(rt.abort(ExitCode::ErrIllegalState, "channel already settling"));
            }

            st.settling_at = rt.curr_epoch() + SETTLE_DELAY;
            if st.settling_at < st.min_settle_height {
                st.settling_at = st.min_settle_height;
            }

            Ok(())
        })
    }

    pub fn collect<BS, RT>(rt: &RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let st: State = rt.state();
        rt.validate_immediate_caller_is([st.from.clone(), st.to.clone()].iter());

        if st.settling_at == ChainEpoch(0) || rt.curr_epoch() < st.settling_at {
            return Err(rt.abort(
                ExitCode::ErrForbidden,
                "payment channel not settling or settled",
            ));
        }

        // TODO revisit: Spec doesn't check this, could be possible balance is below to_send?
        let rem_bal = rt
            .current_balance()
            .checked_sub(&st.to_send)
            .ok_or_else(|| {
                rt.abort(
                    ExitCode::ErrInsufficientFunds,
                    "Cannot send more than remaining balance",
                )
            })?;

        // send remaining balance to `from`
        rt.send::<Ipld>(
            &st.from,
            MethodNum(METHOD_SEND as u64),
            &Serialized::default(),
            &rem_bal,
        )?;

        // send ToSend to `to`
        rt.send::<Ipld>(
            &st.to,
            MethodNum(METHOD_SEND as u64),
            &Serialized::default(),
            &st.to_send,
        )?;

        rt.transaction(|st: &mut State| {
            st.to_send = TokenAmount::new(0);

            Ok(())
        })
    }
}

#[inline]
fn find_lane(lanes: &[LaneState], id: u64) -> (usize, bool) {
    match lanes.binary_search_by(|lane| lane.id.cmp(&id)) {
        Ok(idx) => (idx, true),
        Err(idx) => (idx, false),
    }
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
                Self::constructor(rt, params.deserialize().unwrap())?;
                Ok(Serialized::default())
            }
            Some(Method::Settle) => {
                check_empty_params(params)?;
                Self::settle(rt)?;
                Ok(Serialized::default())
            }
            Some(Method::Collect) => {
                check_empty_params(params)?;
                Self::collect(rt)?;
                Ok(Serialized::default())
            }
            _ => Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method")),
        }
    }
}

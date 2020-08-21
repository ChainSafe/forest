// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;

pub use self::state::{LaneState, Merge, State};
pub use self::types::*;
use crate::{check_empty_params, ACCOUNT_ACTOR_CODE_ID, INIT_ACTOR_CODE_ID};
use address::Address;
use ipld_blockstore::BlockStore;
use num_bigint::{BigInt, Sign};
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Zero};
use runtime::{ActorCode, Runtime};
use vm::{
    actor_error, ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR,
    METHOD_SEND,
};

// * Updated to specs-actors commit: 2a207366baa881a599fe246dc7862eaa774be2f8 (0.8.6)

/// Payment Channel actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    UpdateChannelState = 2,
    Settle = 3,
    Collect = 4,
}

/// Payment Channel actor
pub struct Actor;
impl Actor {
    /// Constructor for Payment channel actor
    pub fn constructor<BS, RT>(rt: &mut RT, params: ConstructorParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // Only InitActor can create a payment channel actor. It creates the actor on
        // behalf of the payer/payee.
        rt.validate_immediate_caller_type(std::iter::once(&*INIT_ACTOR_CODE_ID))?;

        // Check both parties are capable of signing vouchers
        // TODO This was indicated as incorrect, fix when updated on specs-actors
        let to = Self::resolve_account(rt, &params.to)
            .map_err(|e| actor_error!(ErrIllegalArgument; e.msg()))?;

        let from = Self::resolve_account(rt, &params.from)
            .map_err(|e| actor_error!(ErrIllegalArgument; e.msg()))?;

        rt.create(&State::new(from, to))?;
        Ok(())
    }

    /// Resolves an address to a canonical ID address and requires it to address an account actor.
    /// The account actor constructor checks that the embedded address is associated with an appropriate key.
    /// An alternative (more expensive) would be to send a message to the actor to fetch its key.
    fn resolve_account<BS, RT>(rt: &RT, raw: &Address) -> Result<Address, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let resolved = rt
            .resolve_address(raw)?
            .ok_or_else(|| actor_error!(ErrNotFound; "failed to resolve address {}", raw))?;

        let code_cid = rt
            .get_actor_code_cid(&resolved)?
            .ok_or_else(|| actor_error!(ErrIllegalState; "no code for address {}", raw))?;

        if code_cid != *ACCOUNT_ACTOR_CODE_ID {
            Err(
                actor_error!(ErrForbidden; "actor {} must be an account ({}), was {}",
                    raw, *ACCOUNT_ACTOR_CODE_ID, code_cid
                ),
            )
        } else {
            Ok(resolved)
        }
    }

    pub fn update_channel_state<BS, RT>(
        rt: &mut RT,
        params: UpdateChannelStateParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let st: State = rt.state()?;

        rt.validate_immediate_caller_is([st.from, st.to].iter())?;
        let signer = if rt.message().caller() == &st.from {
            st.to
        } else {
            st.from
        };
        let sv = params.sv;

        // Pull signature from signed voucher
        let sig = sv
            .signature
            .as_ref()
            .ok_or_else(|| actor_error!(ErrIllegalArgument; "voucher has no signature"))?;

        // Generate unsigned bytes
        let sv_bz = sv.signing_bytes().map_err(
            |e| actor_error!(ErrIllegalArgument; "failed to serialized SignedVoucher: {}", e),
        )?;

        // Validate signature
        rt.syscalls()
            .verify_signature(&sig, &signer, &sv_bz)
            .map_err(|e| match e.downcast::<ActorError>() {
                Ok(actor_err) => *actor_err,
                Err(other) => {
                    actor_error!(ErrIllegalArgument; "voucher signature invalid: {}", other)
                }
            })?;

        let pch_addr = rt.message().receiver();
        if pch_addr != &sv.channel_addr {
            return Err(actor_error!(ErrIllegalArgument;
                    "voucher payment channel address {} does not match receiver {}",
                    sv.channel_addr, pch_addr));
        }

        if rt.curr_epoch() < sv.time_lock_min {
            return Err(actor_error!(ErrIllegalArgument; "cannot use this voucher yet"));
        }

        if sv.time_lock_max != 0 && rt.curr_epoch() > sv.time_lock_max {
            return Err(actor_error!(ErrIllegalArgument; "this voucher has expired"));
        }

        if sv.amount.sign() == Sign::Minus {
            return Err(actor_error!(ErrIllegalArgument;
                    "voucher amount must be non-negative, was {}", sv.amount));
        }

        if !sv.secret_pre_image.is_empty() {
            let hashed_secret: &[u8] = &rt
                .syscalls()
                .hash_blake2b(&params.secret)
                .map_err(|e| *e.downcast::<ActorError>().unwrap())?;
            if hashed_secret != sv.secret_pre_image.as_slice() {
                return Err(actor_error!(ErrIllegalArgument; "incorrect secret"));
            }
        }

        if let Some(extra) = &sv.extra {
            rt.send(
                extra.actor,
                extra.method,
                Serialized::serialize(PaymentVerifyParams {
                    extra: extra.data.clone(),
                    proof: params.proof,
                })?,
                TokenAmount::from(0u8),
            )
            .map_err(|e| e.wrap("spend voucher verification failed"))?;
        }

        let curr_bal = rt.current_balance()?;
        rt.transaction(|st: &mut State, _| {
            let mut lane_found = true;
            // Find the voucher lane, create and insert it in sorted order if necessary.
            let (idx, exists) = find_lane(&st.lane_states, sv.lane);
            if !exists {
                if st.lane_states.len() >= LANE_LIMIT {
                    return Err(actor_error!(ErrIllegalArgument; "lane limit exceeded"));
                }
                let tmp_ls = LaneState {
                    id: sv.lane,
                    redeemed: BigInt::zero(),
                    nonce: 0,
                };
                st.lane_states.insert(idx, tmp_ls);
                lane_found = false;
            };

            if lane_found && st.lane_states[idx].nonce >= sv.nonce {
                return Err(actor_error!(ErrIllegalArgument;
                        "voucher has an outdated nonce, existing: {}, voucher: {}, cannot redeem",
                        st.lane_states[idx].nonce, sv.nonce));
            }

            // The next section actually calculates the payment amounts to update
            // the payment channel state
            // 1. (optional) sum already redeemed value of all merging lanes
            let mut redeemed = BigInt::default();
            for merge in sv.merges {
                if merge.lane == sv.lane {
                    return Err(actor_error!(ErrIllegalArgument;
                        "voucher cannot merge lanes into it's own lane".to_owned()));
                }
                let (other_idx, exists) = find_lane(&st.lane_states, merge.lane);
                if exists {
                    if st.lane_states[other_idx].nonce >= merge.nonce {
                        return Err(actor_error!(ErrIllegalArgument;
                            "merged lane in voucher has outdated nonce, cannot redeem"));
                    }

                    redeemed += &st.lane_states[other_idx].redeemed;
                    st.lane_states[other_idx].nonce = merge.nonce;
                } else {
                    return Err(actor_error!(ErrIllegalArgument;
                        "voucher specifies invalid merge lane {}", merge.lane));
                }
            }

            // 2. To prevent double counting, remove already redeemed amounts (from
            // voucher or other lanes) from the voucher amount
            st.lane_states[idx].nonce = sv.nonce;
            let balance_delta = &sv.amount - (redeemed + &st.lane_states[idx].redeemed);

            // 3. set new redeemed value for merged-into lane
            st.lane_states[idx].redeemed = sv.amount;

            // 4. check operation validity
            let new_send_balance = balance_delta + &st.to_send;

            if new_send_balance < TokenAmount::from(0) {
                return Err(actor_error!(ErrIllegalArgument;
                    "voucher would leave channel balance negative"));
            }

            if new_send_balance > curr_bal {
                return Err(actor_error!(ErrIllegalArgument;
                    "not enough funds in channel to cover voucher"));
            }

            // 5. add new redemption ToSend
            st.to_send = new_send_balance;

            // update channel settlingAt and MinSettleHeight if delayed by voucher
            if sv.min_settle_height != 0 {
                if st.settling_at != 0 && st.settling_at < sv.min_settle_height {
                    st.settling_at = sv.min_settle_height;
                }
                if st.min_settle_height < sv.min_settle_height {
                    st.min_settle_height = sv.min_settle_height;
                }
            }
            Ok(())
        })?
    }

    pub fn settle<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.transaction(|st: &mut State, rt| {
            rt.validate_immediate_caller_is([st.from, st.to].iter())?;

            if st.settling_at != 0 {
                return Err(actor_error!(ErrIllegalState; "channel already settling"));
            }

            st.settling_at = rt.curr_epoch() + SETTLE_DELAY;
            if st.settling_at < st.min_settle_height {
                st.settling_at = st.min_settle_height;
            }

            Ok(())
        })?
    }

    pub fn collect<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let st: State = rt.state()?;
        rt.validate_immediate_caller_is(&[st.from, st.to])?;

        if st.settling_at == 0 || rt.curr_epoch() < st.settling_at {
            return Err(actor_error!(ErrForbidden; "payment channel not settling or settled"));
        }

        // send ToSend to `to`
        rt.send(st.to, METHOD_SEND, Serialized::default(), st.to_send)
            .map_err(|e| e.wrap("Failed to send funds to `to` address"))?;

        // the remaining balance will be returned to "From" upon deletion.
        rt.delete_actor(&st.from)?;

        Ok(())
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
                Self::constructor(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::UpdateChannelState) => {
                Self::update_channel_state(rt, params.deserialize()?)?;
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
            _ => Err(actor_error!(SysErrInvalidMethod; "Invalid method")),
        }
    }
}

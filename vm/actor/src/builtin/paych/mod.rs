// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;

pub use self::state::{LaneState, Merge, State};
pub use self::types::*;
use crate::{ACCOUNT_ACTOR_CODE_ID, INIT_ACTOR_CODE_ID};
use address::Address;
use cid::Cid;
use encoding::to_vec;
use ipld_blockstore::BlockStore;
use message::Message;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
// use serde::{Deserialize, Deserializer, Serialize, Serializer};
use clock::ChainEpoch;
use vm::{ActorError, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

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
            .map_err(|e| rt.abort(ExitCode::ErrIllegalArgument, e.to_string()))?;

        let from = Self::resolve_account(rt, &params.from)
            .map_err(|e| rt.abort(ExitCode::ErrIllegalArgument, e.to_string()))?;

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
            .ok_or(rt.abort(ExitCode::ErrIllegalArgument, "voucher has no signature"))?;

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

        if sv.secret_pre_image.len() > 0 {
            let hashed_secret: &[u8] = &rt.syscalls().hash_blake2b(&params.secret);
            if hashed_secret != sv.secret_pre_image.as_slice() {
                return Err(rt.abort(ExitCode::ErrIllegalArgument, "incorrect secret"));
            }
        }

        todo!()
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
            // Some(Method::EpochTick) => {
            //     assert_empty_params(params);
            //     Self::epoch_tick(rt)?;
            //     Ok(empty_return())
            // }
            _ => Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method")),
        }
    }
}

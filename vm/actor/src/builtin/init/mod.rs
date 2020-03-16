// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod params;
mod state;

pub use self::params::*;
pub use self::state::State;
use crate::{
    empty_return, make_map, ACCOUNT_ACTOR_CODE_ID, INIT_ACTOR_CODE_ID, MARKET_ACTOR_CODE_ID,
    MINER_ACTOR_CODE_ID, POWER_ACTOR_CODE_ID, SYSTEM_ACTOR_ADDR,
};
use address::Address;
use cid::Cid;
use forest_ipld::Ipld;
use ipld_blockstore::BlockStore;
use message::Message;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

/// Init actor methods available
#[derive(FromPrimitive)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    Exec = 2,
}

impl Method {
    /// from_method_num converts a method number into an Method enum
    fn from_method_num(m: MethodNum) -> Option<Method> {
        FromPrimitive::from_u64(u64::from(m))
    }
}

/// Init actor
pub struct Actor;
impl Actor {
    /// Init actor constructor
    pub fn constructor<BS, RT>(rt: &RT, params: ConstructorParams)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let sys_ref: &Address = &SYSTEM_ACTOR_ADDR;
        rt.validate_immediate_caller_is(std::iter::once(sys_ref));
        let mut empty_map = make_map(rt.store());
        let root = match empty_map.flush() {
            Ok(cid) => cid,
            Err(e) => {
                rt.abort(
                    ExitCode::ErrIllegalState,
                    format!("failed to construct state: {}", e),
                );
                unreachable!()
            }
        };
        rt.create(&State::new(root, params.network_name));
    }

    /// Exec init actor
    pub fn exec<BS, RT>(rt: &RT, params: ExecParams) -> ExecReturn
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any();
        let caller_code = rt
            .get_actor_code_cid(rt.message().from())
            .expect("no code for actor");
        if !can_exec(&caller_code, &params.code_cid) {
            rt.abort(
                ExitCode::ErrForbidden,
                format!(
                    "called type {} cannot exec actor type {}",
                    &caller_code, &params.code_cid
                ),
            )
        }

        // Compute a re-org-stable address.
        // This address exists for use by messages coming from outside the system, in order to
        // stably address the newly created actor even if a chain re-org causes it to end up with
        // a different ID.
        let robust_address = rt.new_actor_address();

        // Allocate an ID for this actor.
        // Store mapping of pubkey or actor address to actor ID
        let id_address: Address = rt.transaction::<State, _, _>(|s| {
            match s.map_address_to_new_id(rt.store(), &robust_address) {
                Ok(a) => a,
                Err(e) => {
                    rt.abort(ExitCode::ErrIllegalState, format!("exec failed {}", e));
                    unreachable!()
                }
            }
        });

        // Create an empty actor
        rt.create_actor(&params.code_cid, &id_address);

        // Invoke constructor
        let (_, exit_code) = rt.send::<Ipld>(
            &id_address,
            MethodNum::new(METHOD_CONSTRUCTOR as u64),
            &params.constructor_params,
            rt.message().value(),
        );

        if !exit_code.is_success() {
            rt.abort(exit_code, "constructor failed".to_owned());
        }

        ExecReturn {
            id_address,
            robust_address,
        }
    }
}

impl ActorCode for Actor {
    fn invoke_method<BS, RT>(&self, rt: &RT, method: MethodNum, params: &Serialized) -> Serialized
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match Method::from_method_num(method) {
            Some(Method::Constructor) => {
                Self::constructor(rt, params.deserialize().unwrap());
                empty_return()
            }
            Some(Method::Exec) => {
                Serialized::serialize(Self::exec(rt, params.deserialize().unwrap())).unwrap()
            }
            _ => {
                // Method number does not match available, abort in runtime
                rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method".to_owned());
                unreachable!();
            }
        }
    }
}

fn can_exec(caller: &Cid, exec: &Cid) -> bool {
    let (acc_ref, init_ref, power_ref, mar_ref, miner_ref): (&Cid, &Cid, &Cid, &Cid, &Cid) = (
        &ACCOUNT_ACTOR_CODE_ID,
        &INIT_ACTOR_CODE_ID,
        &POWER_ACTOR_CODE_ID,
        &MARKET_ACTOR_CODE_ID,
        &MINER_ACTOR_CODE_ID,
    );
    // TODO spec also checks for an undefined Cid, see if this should be supported
    if exec == acc_ref
        || exec == init_ref
        || exec == power_ref
        || exec == mar_ref
        || exec == miner_ref
    {
        exec == miner_ref && caller == power_ref
    } else {
        true
    }
}

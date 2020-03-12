// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use num_bigint::BigUint;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

/// State of storage power actor
pub struct StoragePowerActorState {
    // TODO add power tables on finished spec
    _total_storage: BigUint,
}

/// Method definitions for Storage Power Actor
#[derive(FromPrimitive)]
pub enum StoragePowerMethod {
    /// Constructor for Storage Power Actor
    Constructor = METHOD_CONSTRUCTOR,
    // TODO add other methods on finished spec
    /// Gets the total storage for the network
    GetTotalStorage = 5,
}

impl StoragePowerMethod {
    /// from_method_num converts a method number into an StoragePowerMethod enum
    fn from_method_num(m: MethodNum) -> Option<StoragePowerMethod> {
        FromPrimitive::from_u64(u64::from(m))
    }
}

#[derive(Clone)]
pub struct StoragePowerActorCode;
impl StoragePowerActorCode {
    /// Constructor for StoragePower actor
    fn constructor<RT: Runtime>(_rt: &RT) {
        // TODO
        todo!();
    }
    /// Withdraw available funds from StoragePower map
    fn get_total_storage<RT: Runtime>(_rt: &RT) {
        // TODO
        todo!()
    }
}

impl ActorCode for StoragePowerActorCode {
    fn invoke_method<RT: Runtime>(&self, rt: &RT, method: MethodNum, _params: &Serialized) {
        match StoragePowerMethod::from_method_num(method) {
            // TODO determine parameters for each method on finished spec
            Some(StoragePowerMethod::Constructor) => Self::constructor(rt),
            Some(StoragePowerMethod::GetTotalStorage) => Self::get_total_storage(rt),
            _ => {
                rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method".to_owned());
                unreachable!();
            }
        }
    }
}

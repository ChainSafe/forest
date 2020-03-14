// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::FIRST_NON_SINGLETON_ADDR;
use vm::{
    ActorID, CodeID, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR, METHOD_PLACEHOLDER,
};

use address::Address;
use cid::Cid;
use ipld_blockstore::BlockStore;
use ipld_hamt::{Error as HamtError, Hamt};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use serde::{Deserialize, Serialize};

/// InitActorState is reponsible for creating
// TODO implement actual serialize and deserialize to match
#[derive(Serialize, Deserialize)]
pub struct InitActorState {
    address_map: Cid,
    next_id: ActorID,
}

impl InitActorState {
    pub fn new(address_map: Cid) -> Self {
        Self {
            address_map,
            next_id: FIRST_NON_SINGLETON_ADDR,
        }
    }
    /// Assigns next available ID and incremenets the next_id value from state
    pub fn map_address_to_new_id<BS: BlockStore>(
        &mut self,
        store: &BS,
        addr: &Address,
    ) -> Result<Address, HamtError> {
        let id = self.next_id;
        self.next_id += 1;

        let mut map: Hamt<String, _> = Hamt::load_with_bit_width(&self.address_map, store, 5)?;
        map.set(String::from_utf8_lossy(&addr.to_bytes()).to_string(), id)?;
        self.address_map = map.flush()?;

        Ok(Address::new_id(id.0).expect("Id Address should be created without Error"))
    }

    /// Resolve address
    pub fn resolve_address<BS: BlockStore>(
        &self,
        _store: &BS,
        _addr: &Address,
    ) -> Result<Address, String> {
        // TODO implement address resolution
        todo!()
    }
}

#[derive(FromPrimitive)]
pub enum InitMethod {
    Constructor = METHOD_CONSTRUCTOR,
    Exec = METHOD_PLACEHOLDER,
    GetActorIDForAddress = METHOD_PLACEHOLDER + 1,
}

impl InitMethod {
    /// from_method_num converts a method number into an InitMethod enum
    fn from_method_num(m: MethodNum) -> Option<InitMethod> {
        FromPrimitive::from_u64(u64::from(m))
    }
}

pub struct InitActorCode;
impl InitActorCode {
    fn constructor<RT: Runtime>(_rt: &RT) {
        // Acquire state
        // Update actor substate
    }
    fn exec<RT: Runtime>(_rt: &RT, _code: CodeID, _params: &Serialized) {
        todo!()
    }
    fn get_actor_id_for_address<RT: Runtime>(_rt: &RT, _address: Address) {
        // TODO
        todo!()
    }
}

impl ActorCode for InitActorCode {
    fn invoke_method<RT: Runtime>(&self, rt: &RT, method: MethodNum, params: &Serialized) {
        // Create mutable copy of params for usage in functions
        let params: &mut Serialized = &mut params.clone();
        match InitMethod::from_method_num(method) {
            Some(InitMethod::Constructor) => {
                // TODO unfinished spec

                Self::constructor(rt)
            }
            Some(InitMethod::Exec) => {
                // TODO deserialize CodeID on finished spec
                Self::exec(rt, CodeID::Init, params)
            }
            Some(InitMethod::GetActorIDForAddress) => {
                // Unmarshall address parameter
                // TODO unfinished spec

                // Errors checked, get actor by address
                Self::get_actor_id_for_address(rt, Address::default())
            }
            _ => {
                // Method number does not match available, abort in runtime
                rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method".to_owned());
                unreachable!();
            }
        }
    }
}

#[cfg(test)]
mod test {

    #[test]
    fn assign_id() {
        // TODO replace with new functionality test on full impl
    }
}

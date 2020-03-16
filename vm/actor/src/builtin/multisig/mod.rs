// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;

pub use self::state::State;
pub use self::types::*;
use crate::{empty_return, INIT_ACTOR_ADDR};
use address::Address;
use ipld_blockstore::BlockStore;
use ipld_hamt::Hamt;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR};

/// Multisig actor methods available
#[derive(FromPrimitive)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    Propose = 2,
    Approve = 3,
    Cancel = 4,
    // TODO verify on finished spec this not needed
    // ClearCompleted = 5,
    AddSigner = 6,
    RemoveSigner = 7,
    SwapSigner = 8,
    ChangeNumApprovalsThreshold = 9,
}

impl Method {
    /// from_method_num converts a method number into a Method enum
    fn from_method_num(m: MethodNum) -> Option<Method> {
        FromPrimitive::from_u64(u64::from(m))
    }
}

/// Multisig Actor
pub struct Actor;
impl Actor {
    /// Constructor for Multisig actor
    pub fn constructor<BS, RT>(rt: &RT, params: ConstructorParams)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let sys_ref: &Address = &INIT_ACTOR_ADDR;
        rt.validate_immediate_caller_is(std::iter::once(sys_ref));

        if params.signers.len() < 1 {
            rt.abort(
                ExitCode::ErrIllegalArgument,
                "must have at least one signer".to_owned(),
            );
        }

        // TODO switch to make_map
        let empty_root = match Hamt::<String, _>::new_with_bit_width(rt.store(), 5).flush() {
            Ok(c) => c,
            Err(e) => {
                rt.abort(
                    ExitCode::ErrIllegalState,
                    format!("failed to create empty map: {}", e),
                );
                unreachable!()
            }
        };

        let mut st: State = State {
            signers: params.signers,
            num_approvals_threshold: params.num_approvals_threshold,
            pending_txs: empty_root,
            initial_balance: TokenAmount::new(0),
            next_tx_id: Default::default(),
            start_epoch: Default::default(),
            unlock_duration: Default::default(),
        };

        if params.unlock_duration != 0 {
            st.initial_balance = rt.message().
        }
    }

    /// Multisig actor propose function
    pub fn propose<BS, RT>(_rt: &RT, _params: ProposeParams) -> TxnID
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        todo!()
    }

    /// Multisig actor approve function
    pub fn approve<BS, RT>(_rt: &RT, _params: TxnIDParams)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        todo!()
    }

    /// Multisig actor cancel function
    pub fn cancel<BS, RT>(_rt: &RT, _params: TxnIDParams)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        todo!()
    }

    /// Multisig actor function to add signers to multisig
    pub fn add_signer<BS, RT>(_rt: &RT, _params: AddSignerParams)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        todo!()
    }

    /// Multisig actor function to remove signers to multisig
    pub fn remove_signer<BS, RT>(_rt: &RT, _params: RemoveSignerParams)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        todo!()
    }

    /// Multisig actor function to swap signers to multisig
    pub fn swap_signer<BS, RT>(_rt: &RT, _params: SwapSignerParams)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        todo!()
    }

    /// Multisig actor function to change number of approvals needed
    pub fn change_num_approvals_threshold<BS, RT>(
        _rt: &RT,
        _params: ChangeNumApprovalsThresholdParams,
    ) where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        todo!()
    }

    #[allow(dead_code)]
    fn approve_transaction<BS, RT>(_rt: &RT, _txn_id: TxnID)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        todo!()
    }

    #[allow(dead_code)]
    fn validate_signer<BS, RT>(_rt: &RT, _address: &Address)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        todo!()
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
            Some(Method::Propose) => {
                Serialized::serialize(Self::propose(rt, params.deserialize().unwrap())).unwrap()
            }
            Some(Method::Approve) => {
                Self::approve(rt, params.deserialize().unwrap());
                empty_return()
            }
            Some(Method::Cancel) => {
                Self::cancel(rt, params.deserialize().unwrap());
                empty_return()
            }
            Some(Method::AddSigner) => {
                Self::add_signer(rt, params.deserialize().unwrap());
                empty_return()
            }
            Some(Method::RemoveSigner) => {
                Self::remove_signer(rt, params.deserialize().unwrap());
                empty_return()
            }
            Some(Method::SwapSigner) => {
                Self::swap_signer(rt, params.deserialize().unwrap());
                empty_return()
            }
            Some(Method::ChangeNumApprovalsThreshold) => {
                Self::change_num_approvals_threshold(rt, params.deserialize().unwrap());
                empty_return()
            }
            _ => {
                rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method".to_owned());
                unreachable!();
            }
        }
    }
}

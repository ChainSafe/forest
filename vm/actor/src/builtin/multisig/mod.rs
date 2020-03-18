// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;

pub use self::state::State;
pub use self::types::*;
use crate::{empty_return, make_map, CALLER_TYPES_SIGNABLE, INIT_ACTOR_ADDR};
use address::Address;
use ipld_blockstore::BlockStore;
use message::Message;
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
    /// Converts a method number into a Method enum
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

        if params.signers.is_empty() {
            rt.abort(
                ExitCode::ErrIllegalArgument,
                "Must have at least one signer".to_owned(),
            );
        }

        let empty_root = match make_map(rt.store()).flush() {
            Ok(c) => c,
            Err(e) => {
                rt.abort(
                    ExitCode::ErrIllegalState,
                    format!("Failed to create empty map: {}", e),
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

        if params.unlock_duration.0 != 0 {
            st.initial_balance = rt.message().value().clone();
            st.unlock_duration = params.unlock_duration;
            st.start_epoch = rt.curr_epoch();
        }
        rt.create(&st);
    }

    /// Multisig actor propose function
    pub fn propose<BS, RT>(rt: &RT, params: ProposeParams) -> TxnID
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter());
        let caller_addr: &Address = rt.message().from();

        let tx_id = rt.transaction::<State, _, _>(|st| {
            Self::validate_signer(rt, &st, &caller_addr);
            let t_id = st.next_tx_id;
            st.next_tx_id.0 += 1;

            if let Err(err) = st.put_pending_transaction(
                rt.store(),
                t_id,
                Transaction {
                    to: params.to,
                    value: params.value,
                    method: params.method,
                    params: params.params,
                    approved: Vec::new(),
                },
            ) {
                rt.abort(
                    ExitCode::ErrIllegalState,
                    format!("Failed to put transaction for reason: {}", err),
                )
            }
            // Return the tx id
            t_id
        });

        // Proposal implicitly includes approval of a transaction
        Self::approve_transaction(rt, tx_id);

        // TODO revisit issue referenced in spec:
        // Note: this ID may not be stable across chain re-orgs.
        // https://github.com/filecoin-project/specs-actors/issues/7

        tx_id
    }

    /// Multisig actor approve function
    pub fn approve<BS, RT>(rt: &RT, params: TxnIDParams)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter());
        let caller_addr: &Address = rt.message().from();

        // Validate signer
        rt.transaction::<State, _, _>(|st| Self::validate_signer(rt, &st, &caller_addr));

        Self::approve_transaction(rt, params.id);
    }

    /// Multisig actor cancel function
    pub fn cancel<BS, RT>(rt: &RT, params: TxnIDParams)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter());
        let caller_addr: &Address = rt.message().from();

        rt.transaction::<State, _, _>(|st| {
            Self::validate_signer(rt, &st, caller_addr);

            // Get transaction to cancel
            let tx = match st.get_pending_transaction(rt.store(), params.id) {
                Ok(tx) => tx,
                Err(e) => {
                    rt.abort(
                        ExitCode::ErrNotFound,
                        format!("Failed to get transaction for cancel: {}", e),
                    );
                    unreachable!()
                }
            };

            // Check to make sure transaction proposer is caller address
            if tx.approved.get(0) != Some(&caller_addr) {
                rt.abort(
                    ExitCode::ErrForbidden,
                    "Cannot cancel another signers transaction".to_owned(),
                );
            }

            // Remove transaction
            if let Err(e) = st.delete_pending_transaction(rt.store(), params.id) {
                rt.abort(
                    ExitCode::ErrIllegalState,
                    format!("Failed to delete transaction for cancel: {}", e),
                );
            }
        });
    }

    /// Multisig actor function to add signers to multisig
    pub fn add_signer<BS, RT>(rt: &RT, params: AddSignerParams)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(rt.message().to()));

        rt.transaction::<State, _, _>(|st| {
            // Check if signer to add is already signer
            if st.is_signer(&params.signer) {
                rt.abort(
                    ExitCode::ErrIllegalArgument,
                    "Party is already a signer".to_owned(),
                )
            }

            // Add signer and increase threshold if set
            st.signers.push(params.signer);
            if params.increase {
                st.num_approvals_threshold += 1;
            }
        });
    }

    /// Multisig actor function to remove signers to multisig
    pub fn remove_signer<BS, RT>(rt: &RT, params: RemoveSignerParams)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(rt.message().to()));

        rt.transaction::<State, _, _>(|st| {
            // Check that signer to remove exists
            if !st.is_signer(&params.signer) {
                rt.abort(ExitCode::ErrNotFound, "Party not found".to_owned())
            }

            if st.signers.len() == 1 {
                rt.abort(
                    ExitCode::ErrForbidden,
                    "Cannot remove only signer".to_owned(),
                );
            }

            // Remove signer from state
            st.signers.retain(|s| s != &params.signer);

            // Decrease approvals threshold if decrease param or below threshold
            if params.decrease || st.signers.len() - 1 < st.num_approvals_threshold as usize {
                st.num_approvals_threshold -= 1;
            }
        });
    }

    /// Multisig actor function to swap signers to multisig
    pub fn swap_signer<BS, RT>(rt: &RT, params: SwapSignerParams)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(rt.message().to()));

        rt.transaction::<State, _, _>(|st| {
            // Check that signer to remove exists
            if !st.is_signer(&params.from) {
                rt.abort(ExitCode::ErrNotFound, "Party not found".to_owned())
            }

            // Check if signer to add is already signer
            if st.is_signer(&params.to) {
                rt.abort(
                    ExitCode::ErrIllegalArgument,
                    "Party already present".to_owned(),
                )
            }

            // Remove signer from state
            st.signers.retain(|s| s != &params.from);

            // Add new signer
            st.signers.push(params.to);
        });
    }

    /// Multisig actor function to change number of approvals needed
    pub fn change_num_approvals_threshold<BS, RT>(
        rt: &RT,
        params: ChangeNumApprovalsThresholdParams,
    ) where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(rt.message().to()));

        rt.transaction::<State, _, _>(|st| {
            // Check if valid threshold value
            if params.new_threshold <= 0 || params.new_threshold as usize > st.signers.len() {
                rt.abort(
                    ExitCode::ErrIllegalArgument,
                    "New threshold value not supported".to_owned(),
                );
            }

            // Update threshold on state
            st.num_approvals_threshold = params.new_threshold
        });
    }

    fn approve_transaction<BS, RT>(rt: &RT, tx_id: TxnID)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // Approval transaction
        let (tx, threshold_met): (Transaction, bool) = rt.transaction::<State, _, _>(|st| {
            let mut txn = match st.get_pending_transaction(rt.store(), tx_id) {
                Ok(t) => t,
                Err(e) => {
                    rt.abort(
                        ExitCode::ErrIllegalState,
                        format!("Failed to get transaction for approval: {}", e),
                    );
                    unreachable!()
                }
            };

            // abort duplicate approval
            for previous_approver in &txn.approved {
                if previous_approver == rt.message().from() {
                    rt.abort(
                        ExitCode::ErrIllegalState,
                        "Already approved this message".to_owned(),
                    )
                }
            }

            // update approved on the transaction
            txn.approved.push(rt.message().from().clone());

            if let Err(e) = st.put_pending_transaction(rt.store(), tx_id, txn.clone()) {
                rt.abort(
                    ExitCode::ErrIllegalState,
                    format!("Failed to put transaction for approval: {}", e),
                );
            }

            // Check if number approvals is met
            if txn.approved.len() >= st.num_approvals_threshold as usize {
                // Ensure sufficient funds
                if let Err(e) =
                    st.check_available(rt.current_balance(), txn.value.clone(), rt.curr_epoch())
                {
                    rt.abort(
                        ExitCode::ErrInsufficientFunds,
                        format!("Insufficient funds unlocked: {}", e),
                    )
                }

                // Delete pending transaction
                if let Err(e) = st.delete_pending_transaction(rt.store(), tx_id) {
                    rt.abort(
                        ExitCode::ErrIllegalState,
                        format!("failed to delete transaction for cleanup: {}", e),
                    )
                }

                (txn, true)
            } else {
                // Number of approvals required not met, do not relay message
                (txn, false)
            }
        });

        // Sufficient number of approvals have arrived, relay message
        if threshold_met {
            rt.send::<Serialized>(&tx.to, tx.method, &tx.params, &tx.value);
        }
    }

    fn validate_signer<BS, RT>(rt: &RT, st: &State, address: &Address)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if !st.is_signer(address) {
            rt.abort(ExitCode::ErrForbidden, "Party not a signer".to_owned())
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

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;

pub use self::state::*;
pub use self::types::*;
use crate::{make_map, make_map_with_root, Map, CALLER_TYPES_SIGNABLE, INIT_ACTOR_ADDR};
use address::{Address, Protocol};
use encoding::to_vec;
use ipld_blockstore::BlockStore;
use num_bigint::Sign;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime, Syscalls};
use std::collections::HashSet;
use std::error::Error as StdError;
use vm::{
    actor_error, ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR,
};

/// Multisig actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    Propose = 2,
    Approve = 3,
    Cancel = 4,
    AddSigner = 5,
    RemoveSigner = 6,
    SwapSigner = 7,
    ChangeNumApprovalsThreshold = 8,
}

/// Multisig Actor
pub struct Actor;
impl Actor {
    /// Constructor for Multisig actor
    pub fn constructor<BS, RT>(rt: &mut RT, params: ConstructorParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*INIT_ACTOR_ADDR))?;

        if params.signers.is_empty() {
            return Err(actor_error!(ErrIllegalArgument; "Must have at least one signer"));
        }

        // do not allow duplicate signers
        let mut resolved_signers = HashSet::with_capacity(params.signers.len());
        for signer in &params.signers {
            let resolved = resolve(rt, signer)?;
            if resolved_signers.contains(&resolved) {
                return Err(
                    actor_error!(ErrIllegalArgument; "duplicate signer not allowed: {}", signer),
                );
            }
            resolved_signers.insert(resolved);
        }

        if params.num_approvals_threshold > params.signers.len() as u64 {
            return Err(
                actor_error!(ErrIllegalArgument; "must not require more approvals than signers"),
            );
        }

        if params.num_approvals_threshold < 1 {
            return Err(actor_error!(ErrIllegalArgument; "must require at least one approval"));
        }

        if params.unlock_duration < 0 {
            return Err(actor_error!(ErrIllegalArgument; "negative unlock duration disallowed"));
        }

        let empty_root = make_map(rt.store())
            .flush()
            .map_err(|err| actor_error!(ErrIllegalState; "Failed to create empty map: {}", err))?;

        let mut st: State = State {
            signers: params.signers,
            num_approvals_threshold: params.num_approvals_threshold,
            pending_txs: empty_root,
            initial_balance: TokenAmount::from(0),
            next_tx_id: Default::default(),
            start_epoch: Default::default(),
            unlock_duration: Default::default(),
        };

        if params.unlock_duration != 0 {
            st.initial_balance = rt.message().value_received().clone();
            st.unlock_duration = params.unlock_duration;
            st.start_epoch = rt.curr_epoch();
        }
        rt.create(&st)?;

        Ok(())
    }

    /// Multisig actor propose function
    pub fn propose<BS, RT>(rt: &mut RT, params: ProposeParams) -> Result<ProposeReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;
        let caller_addr: Address = *rt.message().caller();

        if params.value.sign() == Sign::Minus {
            return Err(
                actor_error!(ErrIllegalArgument; "proposed value must be non-negative, was {}", params.value),
            );
        }

        let (txn_id, txn) = rt.transaction(|st: &mut State, rt| {
            if !is_signer(rt, st, &caller_addr)? {
                return Err(actor_error!(ErrForbidden; "{} is not a signer", caller_addr));
            }

            let mut ptx = make_map_with_root(&st.pending_txs, rt.store()).map_err(
                |e| actor_error!(ErrIllegalState; "failed to load pending transactions: {}", e),
            )?;

            let t_id = st.next_tx_id;
            st.next_tx_id.0 += 1;

            let txn = Transaction {
                to: params.to,
                value: params.value,
                method: params.method,
                params: params.params,
                approved: Vec::new(),
            };

            ptx.set(t_id.key(), txn.clone()).map_err(
                |e| actor_error!(ErrIllegalState; "failed to put transaction for propose: {}", e),
            )?;

            st.pending_txs = ptx.flush().map_err(
                |e| actor_error!(ErrIllegalState; "failed to flush pending transactions: {}", e),
            )?;

            Ok((t_id, txn))
        })??;

        let (applied, ret, code) = Self::approve_transaction(rt, txn_id, txn)?;

        Ok(ProposeReturn {
            txn_id,
            applied,
            code,
            ret,
        })
    }

    /// Multisig actor approve function
    pub fn approve<BS, RT>(rt: &mut RT, params: TxnIDParams) -> Result<ApproveReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;
        let caller_addr: Address = *rt.message().caller();

        let (applied, ret, code) = rt.transaction(|st: &mut State, rt| {
            if !is_signer(rt, st, &caller_addr)? {
                return Err(actor_error!(ErrForbidden; "{} is not a signer", caller_addr));
            }

            let ptx = make_map_with_root(&st.pending_txs, rt.store()).map_err(
                |e| actor_error!(ErrIllegalState; "failed to load pending transactions: {}", e),
            )?;

            let txn = get_transaction(rt, &ptx, params.id, params.proposal_hash, true)?;

            let (approved, ret, code) =
                execute_transaction_if_approved(rt, &st, params.id, txn.clone())?;
            if !approved {
                // if the transaction hasn't already been approved, "process" the approval
                // and see if the transaction can be executed
                Self::approve_transaction(rt, params.id, txn)
            } else {
                Ok((approved, ret, code))
            }
        })??;

        Ok(ApproveReturn { applied, code, ret })
    }

    /// Multisig actor cancel function
    pub fn cancel<BS, RT>(rt: &mut RT, params: TxnIDParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;
        let caller_addr: Address = *rt.message().caller();

        rt.transaction(|st: &mut State, rt| {
            if !is_signer(rt, st, &caller_addr)? {
                return Err(actor_error!(ErrForbidden; "{} is not a signer", caller_addr));
            }

            let mut ptx = make_map_with_root(&st.pending_txs, rt.store()).map_err(
                |e| actor_error!(ErrIllegalState; "failed to load pending transactions: {}", e),
            )?;

            // Get transaction to cancel
            let tx = get_pending_transaction(&ptx, params.id).map_err(
                |err| actor_error!(ErrNotFound; "Failed to get transaction for cancel: {}", err),
            )?;

            // Check to make sure transaction proposer is caller address
            if tx.approved.get(0) != Some(&caller_addr) {
                return Err(
                    actor_error!(ErrForbidden; "Cannot cancel another signers transaction"),
                );
            }

            let calculated_hash = compute_proposal_hash(&tx, rt.syscalls()).map_err(|e| {
                actor_error!(ErrIllegalState;
                    "failed to compute proposal hash for (tx: {:?}): {}", params.id, e)
            })?;

            if &params.proposal_hash != &calculated_hash {
                return Err(actor_error!(ErrIllegalState; "hash does not match proposal params"));
            }

            ptx.delete(&params.id.key()).map_err(
                |e| actor_error!(ErrIllegalState; "failed to delete pending transaction: {}", e),
            )?;

            st.pending_txs = ptx.flush().map_err(
                |e| actor_error!(ErrIllegalState; "failed to flush pending transactions: {}", e),
            )?;

            Ok(())
        })?
    }

    /// Multisig actor function to add signers to multisig
    pub fn add_signer<BS, RT>(rt: &mut RT, params: AddSignerParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let receiver = *rt.message().receiver();
        rt.validate_immediate_caller_is(std::iter::once(&receiver))?;

        rt.transaction::<State, _, _>(|st, _| {
            // Check if signer to add is already signer
            if st.is_signer(&params.signer) {
                return Err(ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    "Party is already a signer".to_owned(),
                ));
            }

            // Add signer and increase threshold if set
            st.signers.push(params.signer);
            if params.increase {
                st.num_approvals_threshold += 1;
            }

            Ok(())
        })?
    }

    /// Multisig actor function to remove signers to multisig
    pub fn remove_signer<BS, RT>(rt: &mut RT, params: RemoveSignerParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let receiver = *rt.message().receiver();
        rt.validate_immediate_caller_is(std::iter::once(&receiver))?;

        rt.transaction::<State, _, _>(|st, _| {
            // Check that signer to remove exists
            if !st.is_signer(&params.signer) {
                return Err(ActorError::new(
                    ExitCode::ErrNotFound,
                    "Party not found".to_owned(),
                ));
            }

            if st.signers.len() == 1 {
                ActorError::new(
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
            Ok(())
        })?
    }

    /// Multisig actor function to swap signers to multisig
    pub fn swap_signer<BS, RT>(rt: &mut RT, params: SwapSignerParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let receiver = *rt.message().receiver();
        rt.validate_immediate_caller_is(std::iter::once(&receiver))?;

        rt.transaction::<State, _, _>(|st, _| {
            // Check that signer to remove exists
            if !st.is_signer(&params.from) {
                return Err(ActorError::new(
                    ExitCode::ErrNotFound,
                    "Party not found".to_owned(),
                ));
            }

            // Check if signer to add is already signer
            if st.is_signer(&params.to) {
                return Err(ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    "Party already present".to_owned(),
                ));
            }

            // Remove signer from state
            st.signers.retain(|s| s != &params.from);

            // Add new signer
            st.signers.push(params.to);

            Ok(())
        })?
    }

    /// Multisig actor function to change number of approvals needed
    pub fn change_num_approvals_threshold<BS, RT>(
        rt: &mut RT,
        params: ChangeNumApprovalsThresholdParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let receiver = *rt.message().receiver();
        rt.validate_immediate_caller_is(std::iter::once(&receiver))?;

        rt.transaction::<State, _, _>(|st, _| {
            // Check if valid threshold value
            if params.new_threshold <= 0 || params.new_threshold as usize > st.signers.len() {
                return Err(ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    "New threshold value not supported".to_owned(),
                ));
            }

            // Update threshold on state
            st.num_approvals_threshold = params.new_threshold;
            Ok(())
        })?
    }

    fn approve_transaction<BS, RT>(
        rt: &mut RT,
        tx_id: TxnID,
        _transaction: Transaction,
    ) -> Result<(bool, Serialized, ExitCode), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let from = *rt.message().caller();
        let curr_bal = rt.current_balance()?;
        let curr_epoch = rt.curr_epoch();
        // Approval transaction
        let (tx, threshold_met): (Transaction, bool) =
            rt.transaction::<State, _, _>(|st, rt| {
                let mut txn: Transaction = todo!();
                // match st.get_pending_transaction(rt.store(), tx_id) {
                //     Ok(t) => t,
                //     Err(e) => {
                //         return Err(ActorError::new(
                //             ExitCode::ErrIllegalState,
                //             format!("Failed to get transaction for approval: {}", e),
                //         ));
                //     }
                // };

                // abort duplicate approval
                for previous_approver in &txn.approved {
                    if previous_approver == &from {
                        return Err(ActorError::new(
                            ExitCode::ErrIllegalState,
                            "Already approved this message".to_owned(),
                        ));
                    }
                }

                // update approved on the transaction
                txn.approved.push(from);

                if let Err(e) = st.put_pending_transaction(rt.store(), tx_id, txn.clone()) {
                    return Err(ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("Failed to put transaction for approval: {}", e),
                    ));
                }

                // Check if number approvals is met
                if txn.approved.len() >= st.num_approvals_threshold as usize {
                    // Ensure sufficient funds
                    if let Err(e) = st.check_available(curr_bal, &txn.value, curr_epoch) {
                        return Err(ActorError::new(
                            ExitCode::ErrInsufficientFunds,
                            format!("Insufficient funds unlocked: {}", e),
                        ));
                    }

                    // Delete pending transaction
                    if let Err(e) = st.delete_pending_transaction(rt.store(), tx_id) {
                        return Err(ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("failed to delete transaction for cleanup: {}", e),
                        ));
                    }

                    Ok((txn, true))
                } else {
                    // Number of approvals required not met, do not relay message
                    Ok((txn, false))
                }
            })??;

        // Sufficient number of approvals have arrived, relay message
        if threshold_met {
            rt.send(tx.to, tx.method, tx.params, tx.value)?;
        }

        todo!();
    }

    fn validate_signer<BS, RT>(rt: &RT, st: &State, address: &Address) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if !st.is_signer(address) {
            return Err(rt.abort(ExitCode::ErrForbidden, "Party not a signer"));
        }

        Ok(())
    }
}

fn execute_transaction_if_approved<BS, RT>(
    rt: &mut RT,
    st: &State,
    txn_id: TxnID,
    txn: Transaction,
) -> Result<(bool, Serialized, ExitCode), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let mut out = Serialized::default();
    let mut code = ExitCode::Ok;
    let mut applied = false;
    let threshold_met = txn.approved.len() as u64 >= st.num_approvals_threshold;
    if threshold_met {
        st.check_available(rt.current_balance()?, &txn.value, rt.curr_epoch())
            .map_err(
                |e| actor_error!(ErrInsufficientFunds; "insufficient funds unlocked: {}", e),
            )?;

        match rt.send(txn.to, txn.method, txn.params, txn.value) {
            Ok(ser) => {
                out = ser;
            }
            Err(e) => {
                code = e.exit_code();
            }
        }
        applied = true;

        rt.transaction::<State, Result<_, ActorError>, _>(|st, rt| {
            let mut ptx = make_map_with_root(&st.pending_txs, rt.store()).map_err(
                |e| actor_error!(ErrIllegalState; "failed to load pending transactions: {}", e),
            )?;

            ptx.delete(&txn_id.key()).map_err(|e| {
                actor_error!(ErrIllegalState; "failed to delete transaction for cleanup: {}", e)
            })?;

            st.pending_txs = ptx.flush().map_err(
                |e| actor_error!(ErrIllegalState; "failed to flush pending transactions: {}", e),
            )?;
            Ok(())
        })??;
    }

    Ok((applied, out, code))
}

fn get_pending_transaction<'bs, BS: BlockStore>(
    ptx: &Map<'bs, BS>,
    txn_id: TxnID,
) -> Result<Transaction, String> {
    match ptx.get(&txn_id.key()) {
        Ok(Some(tx)) => Ok(tx),
        Ok(None) => Err(format!("failed to find transaction: {}", txn_id.0,)),
        Err(e) => Err(format!("failed to read transaction: {}", e)),
    }
}

fn get_transaction<'bs, BS, RT>(
    rt: &RT,
    ptx: &Map<'bs, BS>,
    txn_id: TxnID,
    proposal_hash: Vec<u8>,
    check_hash: bool,
) -> Result<Transaction, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let txn = get_pending_transaction(ptx, txn_id)
        .map_err(|e| actor_error!(ErrNotFound; "failed to get transaction for approval: {}", e))?;

    if check_hash {
        let calculated_hash = compute_proposal_hash(&txn, rt.syscalls()).map_err(|e| {
            actor_error!(ErrIllegalState;
                "failed to compute proposal hash for (tx: {:?}): {}", txn_id, e)
        })?;

        if &proposal_hash != &calculated_hash {
            return Err(actor_error!(ErrIllegalArgument; "hash does not match proposal params"));
        }
    }

    Ok(txn)
}

fn is_signer<BS, RT>(rt: &RT, st: &State, address: &Address) -> Result<bool, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let candidate_resolved = resolve(rt, address)?;

    for s in &st.signers {
        let signer_resolved = resolve(rt, s)?;
        if signer_resolved == candidate_resolved {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Computes a digest of a proposed transaction. This digest is used to confirm identity
/// of the transaction associated with an ID, which might change under chain re-orgs.
fn compute_proposal_hash(
    txn: &Transaction,
    sys: &dyn Syscalls,
) -> Result<[u8; 32], Box<dyn StdError>> {
    let proposal_hash = ProposalHashData {
        requester: txn.approved.get(0),
        to: &txn.to,
        value: &txn.value,
        method: &txn.method,
        params: &txn.params,
    };
    let data = to_vec(&proposal_hash)
        .map_err(|e| format!("failed to construct multisig approval hash: {}", e))?;

    sys.hash_blake2b(&data)
}

/// Resolves address to ID or returns address as is if it doesn't have an ID address.
fn resolve<BS, RT>(rt: &RT, address: &Address) -> Result<Address, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if address.protocol() != Protocol::ID {
        Ok(rt.resolve_address(address)?.unwrap_or(address.clone()))
    } else {
        Ok(address.clone())
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
            // TODO double check added return values
            Some(Method::Constructor) => {
                Self::constructor(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::Propose) => {
                let res = Self::propose(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::Approve) => {
                Self::approve(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::Cancel) => {
                Self::cancel(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::AddSigner) => {
                Self::add_signer(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::RemoveSigner) => {
                Self::remove_signer(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::SwapSigner) => {
                Self::swap_signer(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::ChangeNumApprovalsThreshold) => {
                Self::change_num_approvals_threshold(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            None => Err(actor_error!(SysErrInvalidMethod; "Invalid method")),
        }
    }
}

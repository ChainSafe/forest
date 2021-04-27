// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;

pub use self::state::*;
pub use self::types::*;
use crate::{
    make_empty_map, make_map_with_root, resolve_to_id_addr, ActorDowncast, Map,
    CALLER_TYPES_SIGNABLE, INIT_ACTOR_ADDR,
};
use address::Address;
use encoding::to_vec;
use fil_types::HAMT_BIT_WIDTH;
use ipld_blockstore::BlockStore;
use num_bigint::Sign;
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Signed};
use runtime::{ActorCode, Runtime, Syscalls};
use std::collections::HashSet;
use std::error::Error as StdError;
use vm::{
    actor_error, ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR,
};

// * Updated to specs-actors commit: 845089a6d2580e46055c24415a6c32ee688e5186 (v3.0.0)

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
    LockBalance = 9,
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

        if params.signers.len() > SIGNERS_MAX {
            return Err(actor_error!(
                ErrIllegalArgument,
                "cannot add more than {} signers",
                SIGNERS_MAX
            ));
        }

        // resolve signer addresses and do not allow duplicate signers
        let mut resolved_signers = Vec::with_capacity(params.signers.len());
        let mut dedup_signers = HashSet::with_capacity(params.signers.len());
        for signer in &params.signers {
            let resolved = resolve_to_id_addr(rt, signer).map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    format!("failed to resolve addr {} to ID addr", signer),
                )
            })?;
            if dedup_signers.contains(&resolved) {
                return Err(
                    actor_error!(ErrIllegalArgument; "duplicate signer not allowed: {}", signer),
                );
            }
            resolved_signers.push(resolved);
            dedup_signers.insert(resolved);
        }

        if params.num_approvals_threshold > params.signers.len() {
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

        let empty_root = make_empty_map::<_, ()>(rt.store(), HAMT_BIT_WIDTH)
            .flush()
            .map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "Failed to create empty map")
            })?;

        let mut st: State = State {
            signers: resolved_signers,
            num_approvals_threshold: params.num_approvals_threshold,
            pending_txs: empty_root,
            initial_balance: TokenAmount::from(0),
            next_tx_id: Default::default(),
            start_epoch: Default::default(),
            unlock_duration: Default::default(),
        };

        if params.unlock_duration != 0 {
            st.set_locked(
                params.start_epoch,
                params.unlock_duration,
                rt.message().value_received().clone(),
            );
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
        let proposer: Address = *rt.message().caller();

        if params.value.sign() == Sign::Minus {
            return Err(actor_error!(
                ErrIllegalArgument,
                "proposed value must be non-negative, was {}",
                params.value
            ));
        }

        let (txn_id, txn) = rt.transaction(|st: &mut State, rt| {
            if !st.is_signer(&proposer) {
                return Err(actor_error!(ErrForbidden, "{} is not a signer", proposer));
            }

            let mut ptx = make_map_with_root(&st.pending_txs, rt.store()).map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to load pending transactions",
                )
            })?;

            let t_id = st.next_tx_id;
            st.next_tx_id.0 += 1;

            let txn = Transaction {
                to: params.to,
                value: params.value,
                method: params.method,
                params: params.params,
                approved: Vec::new(),
            };

            ptx.set(t_id.key(), txn.clone()).map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to put transaction for propose",
                )
            })?;

            st.pending_txs = ptx.flush().map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to flush pending transactions",
                )
            })?;

            Ok((t_id, txn))
        })?;

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
        let approver: Address = *rt.message().caller();

        let id = params.id;
        let (st, txn) = rt.transaction(|st: &mut State, rt| {
            if !st.is_signer(&approver) {
                return Err(actor_error!(ErrForbidden; "{} is not a signer", approver));
            }

            let ptx = make_map_with_root(&st.pending_txs, rt.store()).map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to load pending transactions",
                )
            })?;

            let txn = get_transaction(rt, &ptx, params.id, params.proposal_hash, true)?;

            // Go implementation holds reference to state after transaction so state must be cloned
            // to match to handle possible exit code inconsistency
            Ok((st.clone(), txn.clone()))
        })?;

        let (applied, ret, code) = execute_transaction_if_approved(rt, &st, id, &txn)?;
        if !applied {
            // if the transaction hasn't already been approved, "process" the approval
            // and see if the transaction can be executed
            let (applied, ret, code) = Self::approve_transaction(rt, id, txn)?;
            Ok(ApproveReturn { applied, code, ret })
        } else {
            Ok(ApproveReturn { applied, code, ret })
        }
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
            if !st.is_signer(&caller_addr) {
                return Err(actor_error!(ErrForbidden; "{} is not a signer", caller_addr));
            }

            let mut ptx = make_map_with_root::<_, Transaction>(&st.pending_txs, rt.store())
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        "failed to load pending transactions",
                    )
                })?;

            let (_, tx) = ptx
                .delete(&params.id.key())
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to pop transaction {:?} for cancel", params.id),
                    )
                })?
                .ok_or_else(|| {
                    actor_error!(ErrNotFound, "no such transaction {:?} to cancel", params.id)
                })?;

            // Check to make sure transaction proposer is caller address
            if tx.approved.get(0) != Some(&caller_addr) {
                return Err(
                    actor_error!(ErrForbidden; "Cannot cancel another signers transaction"),
                );
            }

            let calculated_hash = compute_proposal_hash(&tx, rt).map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    format!("failed to compute proposal hash for (tx: {:?})", params.id),
                )
            })?;

            if !params.proposal_hash.is_empty() && params.proposal_hash != calculated_hash {
                return Err(actor_error!(
                    ErrIllegalState,
                    "hash does not match proposal params"
                ));
            }

            st.pending_txs = ptx.flush().map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to flush pending transactions",
                )
            })?;

            Ok(())
        })
    }

    /// Multisig actor function to add signers to multisig
    pub fn add_signer<BS, RT>(rt: &mut RT, params: AddSignerParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let receiver = *rt.message().receiver();
        rt.validate_immediate_caller_is(std::iter::once(&receiver))?;
        let resolved_new_signer = resolve_to_id_addr(rt, &params.signer).map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                format!("failed to resolve address {}", params.signer),
            )
        })?;

        rt.transaction(|st: &mut State, _| {
            if st.signers.len() >= SIGNERS_MAX {
                return Err(actor_error!(
                    ErrForbidden,
                    "cannot add more than {} signers",
                    SIGNERS_MAX
                ));
            }
            if st.is_signer(&resolved_new_signer) {
                return Err(actor_error!(
                    ErrForbidden,
                    "{} is already a signer",
                    resolved_new_signer
                ));
            }

            // Add signer and increase threshold if set
            st.signers.push(resolved_new_signer);
            if params.increase {
                st.num_approvals_threshold += 1;
            }

            Ok(())
        })
    }

    /// Multisig actor function to remove signers to multisig
    pub fn remove_signer<BS, RT>(rt: &mut RT, params: RemoveSignerParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let receiver = *rt.message().receiver();
        rt.validate_immediate_caller_is(std::iter::once(&receiver))?;
        let resolved_old_signer = resolve_to_id_addr(rt, &params.signer).map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                format!("failed to resolve address {}", params.signer),
            )
        })?;

        rt.transaction(|st: &mut State, rt| {
            if !st.is_signer(&resolved_old_signer) {
                return Err(actor_error!(
                    ErrForbidden,
                    "{} is not a signer",
                    resolved_old_signer
                ));
            }

            if st.signers.len() == 1 {
                return Err(actor_error!(ErrForbidden; "Cannot remove only signer"));
            }

            if !params.decrease && st.signers.len() < st.num_approvals_threshold {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "can't reduce signers to {} below threshold {} with decrease=false",
                    st.signers.len(),
                    st.num_approvals_threshold
                ));
            }

            if params.decrease {
                if st.num_approvals_threshold < 2 {
                    return Err(actor_error!(
                        ErrIllegalArgument,
                        "can't decrease approvals from {} to {}",
                        st.num_approvals_threshold,
                        st.num_approvals_threshold - 1
                    ));
                }
                st.num_approvals_threshold -= 1;
            }

            // Remove approvals from removed signer
            st.purge_approvals(rt.store(), &resolved_old_signer)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        "failed to purge approvals of removed signer",
                    )
                })?;
            st.signers.retain(|s| s != &resolved_old_signer);

            Ok(())
        })?;

        Ok(())
    }

    /// Multisig actor function to swap signers to multisig
    pub fn swap_signer<BS, RT>(rt: &mut RT, params: SwapSignerParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let receiver = *rt.message().receiver();
        rt.validate_immediate_caller_is(std::iter::once(&receiver))?;
        let from_resolved = resolve_to_id_addr(rt, &params.from).map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                format!("failed to resolve address {}", params.from),
            )
        })?;
        let to_resolved = resolve_to_id_addr(rt, &params.to).map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                format!("failed to resolve address {}", params.to),
            )
        })?;

        rt.transaction(|st: &mut State, rt| {
            if !st.is_signer(&from_resolved) {
                return Err(actor_error!(ErrForbidden; "{} is not a signer", from_resolved));
            }

            if st.is_signer(&to_resolved) {
                return Err(
                    actor_error!(ErrIllegalArgument; "{} is already a signer", to_resolved),
                );
            }

            // Remove signer from state (retain preserves order of elements)
            st.signers.retain(|s| s != &from_resolved);

            // Add new signer
            st.signers.push(to_resolved);

            st.purge_approvals(rt.store(), &from_resolved)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        "failed to purge approvals of removed signer",
                    )
                })?;
            Ok(())
        })?;

        Ok(())
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

        rt.transaction(|st: &mut State, _| {
            // Check if valid threshold value
            if params.new_threshold == 0 || params.new_threshold > st.signers.len() {
                return Err(actor_error!(ErrIllegalArgument; "New threshold value not supported"));
            }

            // Update threshold on state
            st.num_approvals_threshold = params.new_threshold;
            Ok(())
        })?;

        Ok(())
    }

    /// Multisig actor function to change number of approvals needed
    pub fn lock_balance<BS, RT>(rt: &mut RT, params: LockBalanceParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let receiver = *rt.message().receiver();
        rt.validate_immediate_caller_is(std::iter::once(&receiver))?;

        if params.unlock_duration <= 0 {
            return Err(actor_error!(
                ErrIllegalArgument,
                "unlock duration must be positive"
            ));
        }

        if params.amount.is_negative() {
            return Err(actor_error!(
                ErrIllegalArgument,
                "amount to lock must be positive"
            ));
        }

        rt.transaction(|st: &mut State, _| {
            if st.unlock_duration != 0 {
                return Err(actor_error!(
                    ErrForbidden,
                    "modification of unlock disallowed"
                ));
            }
            st.set_locked(params.start_epoch, params.unlock_duration, params.amount);
            Ok(())
        })?;

        Ok(())
    }

    fn approve_transaction<BS, RT>(
        rt: &mut RT,
        tx_id: TxnID,
        mut txn: Transaction,
    ) -> Result<(bool, Serialized, ExitCode), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        for previous_approver in &txn.approved {
            if previous_approver == rt.message().caller() {
                return Err(actor_error!(
                    ErrForbidden,
                    "{} already approved this message",
                    previous_approver
                ));
            }
        }

        let st = rt.transaction(|st: &mut State, rt| {
            let mut ptx = make_map_with_root(&st.pending_txs, rt.store()).map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to load pending transactions",
                )
            })?;

            // update approved on the transaction
            txn.approved.push(*rt.message().caller());

            ptx.set(tx_id.key(), txn.clone()).map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    format!("failed to put transaction {} for approval", tx_id.0),
                )
            })?;

            st.pending_txs = ptx.flush().map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to flush pending transactions",
                )
            })?;

            // Go implementation holds reference to state after transaction so this must be cloned
            // to match to handle possible exit code inconsistency
            Ok(st.clone())
        })?;

        execute_transaction_if_approved(rt, &st, tx_id, &txn)
    }
}

fn execute_transaction_if_approved<BS, RT>(
    rt: &mut RT,
    st: &State,
    txn_id: TxnID,
    txn: &Transaction,
) -> Result<(bool, Serialized, ExitCode), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let mut out = Serialized::default();
    let mut code = ExitCode::Ok;
    let mut applied = false;
    let threshold_met = txn.approved.len() >= st.num_approvals_threshold;
    if threshold_met {
        st.check_available(rt.current_balance()?, &txn.value, rt.curr_epoch())
            .map_err(|e| {
                actor_error!(ErrInsufficientFunds, "insufficient funds unlocked: {}", e)
            })?;

        match rt.send(txn.to, txn.method, txn.params.clone(), txn.value.clone()) {
            Ok(ser) => {
                out = ser;
            }
            Err(e) => {
                code = e.exit_code();
            }
        }
        applied = true;

        rt.transaction(|st: &mut State, rt| {
            let mut ptx = make_map_with_root::<_, Transaction>(&st.pending_txs, rt.store())
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        "failed to load pending transactions",
                    )
                })?;

            ptx.delete(&txn_id.key()).map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to delete transaction for cleanup",
                )
            })?;

            st.pending_txs = ptx.flush().map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to flush pending transactions",
                )
            })?;
            Ok(())
        })?;
    }

    Ok((applied, out, code))
}

fn get_transaction<'bs, 'm, BS, RT>(
    rt: &RT,
    ptx: &'m Map<'bs, BS, Transaction>,
    txn_id: TxnID,
    proposal_hash: Vec<u8>,
    check_hash: bool,
) -> Result<&'m Transaction, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let txn = ptx
        .get(&txn_id.key())
        .map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                format!("failed to load transaction {:?} for approval", txn_id),
            )
        })?
        .ok_or_else(|| {
            actor_error!(ErrNotFound, "no such transaction {:?} for approval", txn_id)
        })?;

    if check_hash {
        let calculated_hash = compute_proposal_hash(&txn, rt).map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                format!("failed to compute proposal hash for (tx: {:?})", txn_id),
            )
        })?;

        if !proposal_hash.is_empty() && proposal_hash != calculated_hash {
            return Err(actor_error!(
                ErrIllegalArgument,
                "hash does not match proposal params (ensure requester is an ID address)"
            ));
        }
    }

    Ok(txn)
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
        .map_err(|e| ActorError::from(e).wrap("failed to construct multisig approval hash"))?;

    sys.hash_blake2b(&data)
}

impl ActorCode for Actor {
    fn invoke_method<BS, RT>(
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
                Self::constructor(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::Propose) => {
                let res = Self::propose(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::Approve) => {
                let res = Self::approve(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::Cancel) => {
                Self::cancel(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::AddSigner) => {
                Self::add_signer(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::RemoveSigner) => {
                Self::remove_signer(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::SwapSigner) => {
                Self::swap_signer(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::ChangeNumApprovalsThreshold) => {
                Self::change_num_approvals_threshold(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::LockBalance) => {
                Self::lock_balance(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            None => Err(actor_error!(SysErrInvalidMethod, "Invalid method")),
        }
    }
}

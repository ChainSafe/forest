// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::BTreeSet;

use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::address::Address;
use fvm_shared::econ::TokenAmount;
use fvm_shared::error::ExitCode;
use fvm_shared::{HAMT_BIT_WIDTH, METHOD_CONSTRUCTOR};
use num_derive::FromPrimitive;
use num_traits::Zero;

use fil_actors_runtime_v9::cbor::serialize_vec;
use fil_actors_runtime_v9::runtime::{builtins::Type, Primitives, Runtime};
use fil_actors_runtime_v9::{
    actor_error, make_empty_map, make_map_with_root, resolve_to_actor_id, ActorContext, ActorError,
    AsActorError, Map, INIT_ACTOR_ADDR,
};

pub use self::state::*;
pub use self::types::*;

#[cfg(feature = "fil-actor")]
fil_actors_runtime::wasm_trampoline!(Actor);

mod state;
pub mod testing;
mod types;

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
    UniversalReceiverHook = frc42_dispatch::method_hash!("Receive"),
}

/// Multisig Actor
pub struct Actor;
impl Actor {
    /// Constructor for Multisig actor
    pub fn constructor<BS, RT>(rt: &mut RT, params: ConstructorParams) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&INIT_ACTOR_ADDR))?;

        if params.signers.is_empty() {
            return Err(actor_error!(illegal_argument; "Must have at least one signer"));
        }

        if params.signers.len() > SIGNERS_MAX {
            return Err(actor_error!(
                illegal_argument,
                "cannot add more than {} signers",
                SIGNERS_MAX
            ));
        }

        // resolve signer addresses and do not allow duplicate signers
        let mut resolved_signers = Vec::with_capacity(params.signers.len());
        let mut dedup_signers = BTreeSet::new();
        for signer in &params.signers {
            let resolved = resolve_to_actor_id(rt, signer)?;
            if !dedup_signers.insert(resolved) {
                return Err(
                    actor_error!(illegal_argument; "duplicate signer not allowed: {}", signer),
                );
            }
            resolved_signers.push(Address::new_id(resolved));
        }

        if params.num_approvals_threshold > params.signers.len() as u64 {
            return Err(
                actor_error!(illegal_argument; "must not require more approvals than signers"),
            );
        }

        if params.num_approvals_threshold < 1 {
            return Err(actor_error!(illegal_argument; "must require at least one approval"));
        }

        if params.unlock_duration < 0 {
            return Err(actor_error!(illegal_argument; "negative unlock duration disallowed"));
        }

        let empty_root = make_empty_map::<_, ()>(rt.store(), HAMT_BIT_WIDTH)
            .flush()
            .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to create empty map")?;

        let mut st: State = State {
            signers: resolved_signers,
            num_approvals_threshold: params.num_approvals_threshold,
            pending_txs: empty_root,
            initial_balance: TokenAmount::zero(),
            next_tx_id: Default::default(),
            start_epoch: Default::default(),
            unlock_duration: Default::default(),
        };

        if params.unlock_duration != 0 {
            st.set_locked(
                params.start_epoch,
                params.unlock_duration,
                rt.message().value_received(),
            );
        }
        rt.create(&st)?;

        Ok(())
    }

    /// Multisig actor propose function
    pub fn propose<BS, RT>(rt: &mut RT, params: ProposeParams) -> Result<ProposeReturn, ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(&[Type::Account, Type::Multisig])?;
        let proposer: Address = rt.message().caller();

        if params.value.is_negative() {
            return Err(actor_error!(
                illegal_argument,
                "proposed value must be non-negative, was {}",
                params.value
            ));
        }

        let (txn_id, txn) = rt.transaction(|st: &mut State, rt| {
            if !st.is_signer(&proposer) {
                return Err(actor_error!(forbidden, "{} is not a signer", proposer));
            }

            let mut ptx = make_map_with_root(&st.pending_txs, rt.store()).context_code(
                ExitCode::USR_ILLEGAL_STATE,
                "failed to load pending transactions",
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

            ptx.set(t_id.key(), txn.clone()).context_code(
                ExitCode::USR_ILLEGAL_STATE,
                "failed to put transaction for propose",
            )?;

            st.pending_txs = ptx.flush().context_code(
                ExitCode::USR_ILLEGAL_STATE,
                "failed to flush pending transactions",
            )?;

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
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(&[Type::Account, Type::Multisig])?;
        let approver: Address = rt.message().caller();

        let id = params.id;
        let (st, txn) = rt.transaction(|st: &mut State, rt| {
            if !st.is_signer(&approver) {
                return Err(actor_error!(forbidden; "{} is not a signer", approver));
            }

            let ptx = make_map_with_root(&st.pending_txs, rt.store()).context_code(
                ExitCode::USR_ILLEGAL_STATE,
                "failed to load pending transactions",
            )?;

            let txn = get_transaction(rt, &ptx, params.id, params.proposal_hash)?;

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
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(&[Type::Account, Type::Multisig])?;
        let caller_addr: Address = rt.message().caller();

        rt.transaction(|st: &mut State, rt| {
            if !st.is_signer(&caller_addr) {
                return Err(actor_error!(forbidden; "{} is not a signer", caller_addr));
            }

            let mut ptx = make_map_with_root::<_, Transaction>(&st.pending_txs, rt.store())
                .context_code(
                    ExitCode::USR_ILLEGAL_STATE,
                    "failed to load pending transactions",
                )?;

            let (_, tx) = ptx
                .delete(&params.id.key())
                .with_context_code(ExitCode::USR_ILLEGAL_STATE, || {
                    format!("failed to pop transaction {:?} for cancel", params.id)
                })?
                .ok_or_else(|| {
                    actor_error!(not_found, "no such transaction {:?} to cancel", params.id)
                })?;

            // Check to make sure transaction proposer is caller address
            if tx.approved.get(0) != Some(&caller_addr) {
                return Err(actor_error!(forbidden; "Cannot cancel another signers transaction"));
            }

            let calculated_hash = compute_proposal_hash(&tx, rt)
                .with_context_code(ExitCode::USR_ILLEGAL_STATE, || {
                    format!("failed to compute proposal hash for (tx: {:?})", params.id)
                })?;

            if !params.proposal_hash.is_empty() && params.proposal_hash != calculated_hash {
                return Err(actor_error!(
                    illegal_state,
                    "hash does not match proposal params"
                ));
            }

            st.pending_txs = ptx.flush().context_code(
                ExitCode::USR_ILLEGAL_STATE,
                "failed to flush pending transactions",
            )?;

            Ok(())
        })
    }

    /// Multisig actor function to add signers to multisig
    pub fn add_signer<BS, RT>(rt: &mut RT, params: AddSignerParams) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let receiver = rt.message().receiver();
        rt.validate_immediate_caller_is(std::iter::once(&receiver))?;
        let resolved_new_signer = resolve_to_actor_id(rt, &params.signer)?;

        rt.transaction(|st: &mut State, _| {
            if st.signers.len() >= SIGNERS_MAX {
                return Err(actor_error!(
                    forbidden,
                    "cannot add more than {} signers",
                    SIGNERS_MAX
                ));
            }
            if st.is_signer(&Address::new_id(resolved_new_signer)) {
                return Err(actor_error!(
                    forbidden,
                    "{} is already a signer",
                    resolved_new_signer
                ));
            }

            // Add signer and increase threshold if set
            st.signers.push(Address::new_id(resolved_new_signer));
            if params.increase {
                st.num_approvals_threshold += 1;
            }

            Ok(())
        })
    }

    /// Multisig actor function to remove signers to multisig
    pub fn remove_signer<BS, RT>(rt: &mut RT, params: RemoveSignerParams) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let receiver = rt.message().receiver();
        rt.validate_immediate_caller_is(std::iter::once(&receiver))?;
        let resolved_old_signer = resolve_to_actor_id(rt, &params.signer)?;

        rt.transaction(|st: &mut State, rt| {
            if !st.is_signer(&Address::new_id(resolved_old_signer)) {
                return Err(actor_error!(
                    forbidden,
                    "{} is not a signer",
                    resolved_old_signer
                ));
            }

            if st.signers.len() == 1 {
                return Err(actor_error!(forbidden; "Cannot remove only signer"));
            }

            if !params.decrease && ((st.signers.len() - 1) as u64) < st.num_approvals_threshold {
                return Err(actor_error!(
                    illegal_argument,
                    "can't reduce signers to {} below threshold {} with decrease=false",
                    st.signers.len(),
                    st.num_approvals_threshold
                ));
            }

            if params.decrease {
                if st.num_approvals_threshold < 2 {
                    return Err(actor_error!(
                        illegal_argument,
                        "can't decrease approvals from {} to {}",
                        st.num_approvals_threshold,
                        st.num_approvals_threshold - 1
                    ));
                }
                st.num_approvals_threshold -= 1;
            }

            // Remove approvals from removed signer
            st.purge_approvals(rt.store(), &Address::new_id(resolved_old_signer))
                .context("failed to purge approvals of removed signer")?;
            st.signers
                .retain(|s| s != &Address::new_id(resolved_old_signer));

            Ok(())
        })?;

        Ok(())
    }

    /// Multisig actor function to swap signers to multisig
    pub fn swap_signer<BS, RT>(rt: &mut RT, params: SwapSignerParams) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let receiver = rt.message().receiver();
        rt.validate_immediate_caller_is(std::iter::once(&receiver))?;
        let from_resolved = resolve_to_actor_id(rt, &params.from)?;
        let to_resolved = resolve_to_actor_id(rt, &params.to)?;

        rt.transaction(|st: &mut State, rt| {
            if !st.is_signer(&Address::new_id(from_resolved)) {
                return Err(actor_error!(forbidden; "{} is not a signer", from_resolved));
            }

            if st.is_signer(&Address::new_id(to_resolved)) {
                return Err(actor_error!(illegal_argument; "{} is already a signer", to_resolved));
            }

            // Remove signer from state (retain preserves order of elements)
            st.signers.retain(|s| s != &Address::new_id(from_resolved));

            // Add new signer
            st.signers.push(Address::new_id(to_resolved));

            st.purge_approvals(rt.store(), &Address::new_id(from_resolved))?;
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
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let receiver = rt.message().receiver();
        rt.validate_immediate_caller_is(std::iter::once(&receiver))?;

        rt.transaction(|st: &mut State, _| {
            // Check if valid threshold value
            if params.new_threshold == 0 || params.new_threshold > st.signers.len() as u64 {
                return Err(actor_error!(illegal_argument; "New threshold value not supported"));
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
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let receiver = rt.message().receiver();
        rt.validate_immediate_caller_is(std::iter::once(&receiver))?;

        if params.unlock_duration <= 0 {
            return Err(actor_error!(
                illegal_argument,
                "unlock duration must be positive"
            ));
        }

        if params.amount.is_negative() {
            return Err(actor_error!(
                illegal_argument,
                "amount to lock must be positive"
            ));
        }

        rt.transaction(|st: &mut State, _| {
            if st.unlock_duration != 0 {
                return Err(actor_error!(forbidden, "modification of unlock disallowed"));
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
    ) -> Result<(bool, RawBytes, ExitCode), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        for previous_approver in &txn.approved {
            if *previous_approver == rt.message().caller() {
                return Err(actor_error!(
                    forbidden,
                    "{} already approved this message",
                    previous_approver
                ));
            }
        }

        let st = rt.transaction(|st: &mut State, rt| {
            let mut ptx = make_map_with_root(&st.pending_txs, rt.store()).context_code(
                ExitCode::USR_ILLEGAL_STATE,
                "failed to load pending transactions",
            )?;

            // update approved on the transaction
            txn.approved.push(rt.message().caller());

            ptx.set(tx_id.key(), txn.clone())
                .with_context_code(ExitCode::USR_ILLEGAL_STATE, || {
                    format!("failed to put transaction {} for approval", tx_id.0)
                })?;

            st.pending_txs = ptx.flush().context_code(
                ExitCode::USR_ILLEGAL_STATE,
                "failed to flush pending transactions",
            )?;

            // Go implementation holds reference to state after transaction so this must be cloned
            // to match to handle possible exit code inconsistency
            Ok(st.clone())
        })?;

        execute_transaction_if_approved(rt, &st, tx_id, &txn)
    }

    // Always succeeds, accepting any transfers.
    pub fn universal_receiver_hook<BS, RT>(
        rt: &mut RT,
        _params: &RawBytes,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;
        Ok(())
    }
}

fn execute_transaction_if_approved<BS, RT>(
    rt: &mut RT,
    st: &State,
    txn_id: TxnID,
    txn: &Transaction,
) -> Result<(bool, RawBytes, ExitCode), ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let mut out = RawBytes::default();
    let mut code = ExitCode::OK;
    let mut applied = false;
    let threshold_met = txn.approved.len() as u64 >= st.num_approvals_threshold;
    if threshold_met {
        st.check_available(rt.current_balance(), &txn.value, rt.curr_epoch())?;

        match rt.send(&txn.to, txn.method, txn.params.clone(), txn.value.clone()) {
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
                .context_code(
                    ExitCode::USR_ILLEGAL_STATE,
                    "failed to load pending transactions",
                )?;

            ptx.delete(&txn_id.key()).context_code(
                ExitCode::USR_ILLEGAL_STATE,
                "failed to delete transaction for cleanup",
            )?;

            st.pending_txs = ptx.flush().context_code(
                ExitCode::USR_ILLEGAL_STATE,
                "failed to flush pending transactions",
            )?;
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
) -> Result<&'m Transaction, ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let txn = ptx
        .get(&txn_id.key())
        .with_context_code(ExitCode::USR_ILLEGAL_STATE, || {
            format!("failed to load transaction {:?} for approval", txn_id)
        })?
        .ok_or_else(|| actor_error!(not_found, "no such transaction {:?} for approval", txn_id))?;

    if !proposal_hash.is_empty() {
        let calculated_hash = compute_proposal_hash(txn, rt)
            .with_context_code(ExitCode::USR_ILLEGAL_STATE, || {
                format!("failed to compute proposal hash for (tx: {:?})", txn_id)
            })?;

        if proposal_hash != calculated_hash {
            return Err(actor_error!(
                illegal_argument,
                "hash does not match proposal params (ensure requester is an ID address)"
            ));
        }
    }

    Ok(txn)
}

/// Computes a digest of a proposed transaction. This digest is used to confirm identity
/// of the transaction associated with an ID, which might change under chain re-orgs.
pub fn compute_proposal_hash(txn: &Transaction, sys: &dyn Primitives) -> anyhow::Result<[u8; 32]> {
    let proposal_hash = ProposalHashData {
        requester: txn.approved.get(0),
        to: &txn.to,
        value: &txn.value,
        method: &txn.method,
        params: &txn.params,
    };
    let data = serialize_vec(&proposal_hash, "proposal hash")?;
    Ok(sys.hash_blake2b(&data))
}

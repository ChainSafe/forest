// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::utils::structured;
use super::*;
use crate::interpreter::{ExecutionContext, IMPLICIT_MESSAGE_GAS_LIMIT, VM, VMTrace};
use crate::message::{MessageRead as _, MessageReadWrite as _};
use crate::rpc::state::{ApiInvocResult, MessageGasCost};
use crate::shim::executor::ApplyRet;
use crate::shim::message::{METHOD_SEND, Message};
use crate::state_migration::run_state_migrations;
use std::time::Duration;
use tracing::instrument;

impl StateManager {
    #[instrument(skip(self))]
    fn call_raw_blocking(
        &self,
        state_cid: Option<Cid>,
        msg: &Message,
        tipset: Option<Tipset>,
    ) -> Result<ApiInvocResult, Error> {
        let mut msg = msg.clone();
        let chain_config = self.chain_config();

        let tipset = if let Some(ts) = tipset {
            if ts.epoch() > 0 {
                // Explicit-state calls already hold every fork below the tipset epoch, so only a
                // migration at the tipset epoch itself is refused. Parent-state calls (no explicit
                // state) lag a tipset, so a migration at the parent epoch must also be refused.
                let (fork_floor, fork_height) = if state_cid.is_some() {
                    (ts.epoch(), ts.epoch() + 1)
                } else {
                    let parent = self
                        .chain_index()
                        .load_required_tipset(ts.parents())
                        .map_err(Error::other)?;
                    (parent.epoch(), ts.epoch() + 1)
                };
                if let Some(epoch) = chain_config.expensive_fork_between(fork_floor, fork_height) {
                    return Err(Error::ExpensiveFork { epoch });
                }
            }
            ts
        } else {
            // Search back till we find a height with no fork, or we reach the beginning.
            let mut heaviest_ts = self.heaviest_tipset();
            while heaviest_ts.epoch() > 0 {
                let parent = self
                    .chain_index()
                    .load_required_tipset(heaviest_ts.parents())
                    .map_err(Error::other)?;
                if !chain_config.has_expensive_fork_between(parent.epoch(), heaviest_ts.epoch() + 1)
                {
                    break;
                }
                heaviest_ts = parent;
            }
            heaviest_ts
        };

        let state_cid = state_cid.unwrap_or(*tipset.parent_state());

        // Handle state forks
        let state_cid = match run_state_migrations(
            tipset.epoch(),
            self.chain_config(),
            self.db(),
            &state_cid,
        ) {
            Ok(Some(new_state)) => new_state,
            Ok(None) => state_cid,
            Err(e) => return Err(Error::other(e)),
        };

        let height = tipset.epoch();
        let mut vm = VM::new(
            ExecutionContext {
                heaviest_tipset: tipset.shallow_clone(),
                state_tree_root: state_cid,
                epoch: height,
                rand: Box::new(self.chain_rand(tipset.shallow_clone())),
                base_fee: tipset.block_headers().first().parent_base_fee.clone(),
                circ_supply: self.genesis_info().get_vm_circulating_supply(
                    height,
                    self.db(),
                    &state_cid,
                )?,
                chain_config: self.chain_config().shallow_clone(),
                chain_index: self.chain_index().shallow_clone(),
                timestamp: tipset.min_timestamp(),
            },
            &self.engine,
            VMTrace::Traced,
        )?;

        let tipset_messages = self
            .chain_store()
            .messages_for_tipset(&tipset)
            .map_err(|err| Error::Other(err.to_string()))?;

        let prior_messsages = tipset_messages
            .iter()
            .filter(|ts_msg| ts_msg.message().from() == msg.from());

        for m in prior_messsages {
            vm.apply_message(m)?;
        }

        // We flush to get the VM's view of the state tree after applying the above messages
        // This is needed to get the correct nonce from the actor state to match the VM
        let state_cid = vm.flush()?;

        let state = StateTree::new_from_root(self.db(), &state_cid)?;

        let from_actor = state
            .get_actor(&msg.from())?
            .ok_or_else(|| anyhow::anyhow!("actor not found"))?;
        msg.set_sequence(from_actor.sequence);

        // Implicit messages need to set a special gas limit
        msg.gas_limit = IMPLICIT_MESSAGE_GAS_LIMIT as u64;

        let (apply_ret, duration) = vm.apply_implicit_message(&msg)?;

        let msg_cid = msg.cid();
        Ok(ApiInvocResult {
            msg,
            msg_rct: Some(apply_ret.msg_receipt()),
            msg_cid,
            error: apply_ret.failure_info().unwrap_or_default(),
            duration: duration.as_nanos().clamp(0, u128::from(u64::MAX)) as u64,
            gas_cost: MessageGasCost::default(),
            execution_trace: structured::parse_events(apply_ret.exec_trace()).unwrap_or_default(),
        })
    }

    /// runs the given message and returns its result without any persisted
    /// changes.
    pub async fn call(
        &self,
        message: Arc<Message>,
        tipset: Option<Tipset>,
    ) -> Result<ApiInvocResult, Error> {
        let this = self.shallow_clone();
        tokio::task::spawn_blocking(move || this.call_blocking(&message, tipset)).await?
    }

    /// Blocking version of [`Self::call`], use with caution.
    pub fn call_blocking(
        &self,
        message: &Message,
        tipset: Option<Tipset>,
    ) -> Result<ApiInvocResult, Error> {
        self.call_raw_blocking(None, message, tipset)
    }

    /// Same as [`StateManager::call`] but runs the message on the given state and not
    /// on the parent state of the tipset.
    pub async fn call_on_state(
        &self,
        state_cid: Cid,
        message: Arc<Message>,
        tipset: Option<Tipset>,
    ) -> Result<ApiInvocResult, Error> {
        let this = self.shallow_clone();
        tokio::task::spawn_blocking(move || {
            this.call_on_state_blocking(state_cid, &message, tipset)
        })
        .await?
    }

    /// Blocking version of [`Self::call_on_state`], use with caution.
    pub fn call_on_state_blocking(
        &self,
        state_cid: Cid,
        message: &Message,
        tipset: Option<Tipset>,
    ) -> Result<ApiInvocResult, Error> {
        self.call_raw_blocking(Some(state_cid), message, tipset)
    }

    pub async fn apply_on_state_with_gas(
        &self,
        tipset: Option<&Tipset>,
        msg: &Message,
        vm_flush: VMFlush,
        vm_trace: VMTrace,
        sender_validation: SenderValidation,
    ) -> anyhow::Result<(ApiInvocResult, Option<Cid>)> {
        let ts = tipset.map_or_else(|| self.heaviest_tipset(), Tipset::shallow_clone);

        let from_protocol = match sender_validation {
            SenderValidation::Skip => msg.from.protocol(),
            SenderValidation::Enforce => {
                match self.resolve_to_deterministic_address(msg.from, &ts).await {
                    Ok(from) => from.protocol(),
                    Err(e) => {
                        let TipsetState { state_root, .. } = self.load_tipset_state(&ts).await?;
                        return Err(if self.get_actor(&msg.from, state_root)?.is_some() {
                            e.context("could not resolve key")
                        } else {
                            Error::SenderValidationFailed(format!(
                                "sender {} not found on chain",
                                msg.from
                            ))
                            .into()
                        });
                    }
                }
            }
        };
        let chain_msg = ChainMessage::for_gas_estimation(msg.clone(), from_protocol);

        let (apply_ret, duration, state_root) = self
            .call_with_gas(
                chain_msg,
                Default::default(),
                Some(ts),
                vm_flush,
                vm_trace,
                sender_validation,
            )
            .await?;

        let msg_rct = Some(apply_ret.msg_receipt());
        let error = apply_ret.failure_info().unwrap_or_default();
        Ok((
            ApiInvocResult {
                msg_cid: msg.cid(),
                msg: msg.clone(),
                msg_rct,
                error,
                duration: duration.as_nanos().clamp(0, u128::from(u64::MAX)) as u64,
                gas_cost: MessageGasCost::default(),
                execution_trace: structured::parse_events(apply_ret.into_exec_trace())
                    .unwrap_or_default(),
            },
            state_root,
        ))
    }

    /// Computes message on the given [Tipset] state, after applying other
    /// messages and returns the values computed in the VM.
    pub async fn call_with_gas(
        &self,
        mut message: ChainMessage,
        prior_messages: Arc<Vec<ChainMessage>>,
        tipset: Option<Tipset>,
        vm_flush: VMFlush,
        vm_trace: VMTrace,
        sender_validation: SenderValidation,
    ) -> Result<(ApplyRet, Duration, Option<Cid>), Error> {
        let ts = tipset.unwrap_or_else(|| self.heaviest_tipset());
        let TipsetState { state_root, .. } = self
            .load_tipset_state(&ts)
            .await
            .map_err(|e| Error::Other(format!("Could not load tipset state: {e:#}")))?;
        let chain_rand = self.chain_rand(ts.clone());

        // Since we're simulating a future message, pretend we're applying it in the
        // "next" tipset
        let epoch = ts.epoch() + 1;
        let this = self.shallow_clone();
        tokio::task::spawn_blocking(move || {
            // FVM requires a stack size of 64MiB. The alternative is to use `ThreadedExecutor` from
            // FVM, but that introduces some constraints, and possible deadlocks.
            let (ret, duration, state_cid) = stacker::grow(64 << 20, || -> Result<_, Error> {
                let mut vm = VM::new(
                    ExecutionContext {
                        heaviest_tipset: ts.clone(),
                        state_tree_root: state_root,
                        epoch,
                        rand: Box::new(chain_rand),
                        base_fee: ts.block_headers().first().parent_base_fee.clone(),
                        circ_supply: this.genesis_info().get_vm_circulating_supply(
                            epoch,
                            this.chain_index().db(),
                            &state_root,
                        )?,
                        chain_config: this.chain_config().shallow_clone(),
                        chain_index: this.chain_index().shallow_clone(),
                        timestamp: ts.min_timestamp(),
                    },
                    &this.engine,
                    vm_trace,
                )?;

                for msg in prior_messages.iter() {
                    vm.apply_message(msg)?;
                }

                let mut sender_created = false;
                let from_actor = match vm
                    .get_actor(&message.from())
                    .map_err(|e| Error::Other(format!("Could not get actor from state: {e:#}")))?
                {
                    Some(actor) => actor,
                    None => match sender_validation {
                        SenderValidation::Enforce => {
                            return Err(Error::SenderValidationFailed(format!(
                                "sender {} not found on chain",
                                message.from()
                            )));
                        }
                        SenderValidation::Skip => {
                            let (create_ret, _) =
                                vm.apply_implicit_message(&placeholder_send(message.from()))?;
                            let exit_code = create_ret.msg_receipt().exit_code();
                            if !exit_code.is_success() {
                                return Err(Error::Other(format!(
                                    "failed to create ephemeral sender placeholder {} (exit={exit_code}): {}",
                                    message.from(),
                                    create_ret.failure_info().unwrap_or_default()
                                )));
                            }
                            sender_created = true;
                            vm.get_actor(&message.from())
                                .map_err(|e| {
                                    Error::Other(format!("Could not get placeholder actor: {e:#}"))
                                })?
                                .ok_or_else(|| {
                                    Error::Other(format!(
                                        "ephemeral sender placeholder {} missing after creation",
                                        message.from()
                                    ))
                                })?
                        }
                    },
                };

                message.set_sequence(from_actor.sequence);
                let (ret, duration) = match (sender_validation, sender_created) {
                    // An existing non-account sender needs the implicit path, which skips the
                    // account-type, nonce and balance checks.
                    (SenderValidation::Skip, false) => {
                        vm.apply_implicit_message(message.message())?
                    }
                    // A fresh placeholder is a valid sender, so the explicit path keeps gas
                    // accounting, inclusion cost included, matching a real first send.
                    _ => vm.apply_message(&message)?,
                };
                let state_root = match vm_flush {
                    VMFlush::Flush => Some(vm.flush()?),
                    VMFlush::Skip => None,
                };
                Ok((ret, duration, state_root))
            })?;

            Ok((ret, duration, state_cid))
        })
        .await?
    }
}

fn placeholder_send(to: Address) -> Message {
    Message {
        from: Address::SYSTEM_ACTOR,
        to,
        method_num: METHOD_SEND,
        gas_limit: IMPLICIT_MESSAGE_GAS_LIMIT as u64,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_send_is_a_free_zero_value_send_from_the_system_actor() {
        let sender = Address::new_delegated(10, &[7; 20]).unwrap();
        let msg = placeholder_send(sender);

        assert_eq!(msg.from, Address::SYSTEM_ACTOR);
        assert_eq!(msg.to, sender);
        assert_eq!(msg.method_num, METHOD_SEND);
        assert_eq!(msg.gas_limit, IMPLICIT_MESSAGE_GAS_LIMIT as u64);
        assert!(msg.value.is_zero());
        assert!(msg.gas_fee_cap.is_zero());
        assert!(msg.gas_premium.is_zero());
    }
}

// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::circulating_supply::GenesisInfo;
use super::utils::structured;
use super::*;
use crate::interpreter::{ExecutionContext, IMPLICIT_MESSAGE_GAS_LIMIT, VM, VMTrace};
use crate::message::{MessageRead as _, MessageReadWrite as _, SignedMessage};
use crate::rpc::state::{ApiInvocResult, InvocResult, MessageGasCost};
use crate::shim::address::Protocol;
use crate::shim::crypto::{Signature, SignatureType};
use crate::shim::executor::ApplyRet;
use crate::shim::message::Message;
use fvm_shared4::crypto::signature::SECP_SIG_LEN;
use std::time::Duration;
use tracing::instrument;

impl StateManager {
    #[instrument(skip(self, rand))]
    fn call_raw(
        &self,
        state_cid: Option<Cid>,
        msg: &Message,
        rand: ChainRand,
        tipset: &Tipset,
    ) -> Result<ApiInvocResult, Error> {
        let mut msg = msg.clone();

        let state_cid = state_cid.unwrap_or(*tipset.parent_state());

        let tipset_messages = self
            .chain_store()
            .messages_for_tipset(tipset)
            .map_err(|err| Error::Other(err.to_string()))?;

        let prior_messsages = tipset_messages
            .iter()
            .filter(|ts_msg| ts_msg.message().from() == msg.from());

        // Handle state forks

        let height = tipset.epoch();
        let genesis_info = GenesisInfo::from_chain_config(self.chain_config().clone());
        let mut vm = VM::new(
            ExecutionContext {
                heaviest_tipset: tipset.shallow_clone(),
                state_tree_root: state_cid,
                epoch: height,
                rand: Box::new(rand),
                base_fee: tipset.block_headers().first().parent_base_fee.clone(),
                circ_supply: genesis_info.get_vm_circulating_supply(
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
        let mut msg = msg.clone();
        msg.gas_limit = IMPLICIT_MESSAGE_GAS_LIMIT as u64;

        let (apply_ret, duration) = vm.apply_implicit_message(&msg)?;

        Ok(ApiInvocResult {
            msg: msg.clone(),
            msg_rct: Some(apply_ret.msg_receipt()),
            msg_cid: msg.cid(),
            error: apply_ret.failure_info().unwrap_or_default(),
            duration: duration.as_nanos().clamp(0, u128::from(u64::MAX)) as u64,
            gas_cost: MessageGasCost::default(),
            execution_trace: structured::parse_events(apply_ret.exec_trace()).unwrap_or_default(),
        })
    }

    /// runs the given message and returns its result without any persisted
    /// changes.
    pub fn call(&self, message: &Message, tipset: Option<Tipset>) -> Result<ApiInvocResult, Error> {
        let ts = tipset.unwrap_or_else(|| self.heaviest_tipset());
        let chain_rand = self.chain_rand(ts.shallow_clone());
        self.call_raw(None, message, chain_rand, &ts)
    }

    /// Same as [`StateManager::call`] but runs the message on the given state and not
    /// on the parent state of the tipset.
    pub fn call_on_state(
        &self,
        state_cid: Cid,
        message: &Message,
        tipset: Option<Tipset>,
    ) -> Result<ApiInvocResult, Error> {
        let ts = tipset.unwrap_or_else(|| self.cs.heaviest_tipset());
        let chain_rand = self.chain_rand(ts.shallow_clone());
        self.call_raw(Some(state_cid), message, chain_rand, &ts)
    }

    pub async fn apply_on_state_with_gas(
        &self,
        tipset: Option<Tipset>,
        msg: Message,
        vm_flush: VMFlush,
    ) -> anyhow::Result<(ApiInvocResult, Option<Cid>)> {
        let ts = tipset.unwrap_or_else(|| self.heaviest_tipset());

        let from_a = self.resolve_to_key_addr(&msg.from, &ts).await?;

        // Pretend that the message is signed. This has an influence on the gas
        // cost. We obviously can't generate a valid signature. Instead, we just
        // fill the signature with zeros. The validity is not checked.
        let mut chain_msg = match from_a.protocol() {
            Protocol::Secp256k1 => SignedMessage::new_unchecked(
                msg.clone(),
                Signature::new_secp256k1(vec![0; SECP_SIG_LEN]),
            )
            .into(),
            Protocol::Delegated => SignedMessage::new_unchecked(
                msg.clone(),
                // In Lotus, delegated signatures have the same length as SECP256k1.
                // This may or may not change in the future.
                Signature::new(SignatureType::Delegated, vec![0; SECP_SIG_LEN]),
            )
            .into(),
            _ => msg.clone().into(),
        };

        let (_invoc_res, apply_ret, duration, state_root) = self
            .call_with_gas(&mut chain_msg, &[], Some(ts), vm_flush)
            .await?;

        Ok((
            ApiInvocResult {
                msg_cid: msg.cid(),
                msg,
                msg_rct: Some(apply_ret.msg_receipt()),
                error: apply_ret.failure_info().unwrap_or_default(),
                duration: duration.as_nanos().clamp(0, u128::from(u64::MAX)) as u64,
                gas_cost: MessageGasCost::default(),
                execution_trace: structured::parse_events(apply_ret.exec_trace())
                    .unwrap_or_default(),
            },
            state_root,
        ))
    }

    /// Computes message on the given [Tipset] state, after applying other
    /// messages and returns the values computed in the VM.
    pub async fn call_with_gas(
        &self,
        message: &mut ChainMessage,
        prior_messages: &[ChainMessage],
        tipset: Option<Tipset>,
        vm_flush: VMFlush,
    ) -> Result<(InvocResult, ApplyRet, Duration, Option<Cid>), Error> {
        let ts = tipset.unwrap_or_else(|| self.heaviest_tipset());
        let TipsetState { state_root, .. } = self
            .load_tipset_state(&ts)
            .await
            .map_err(|e| Error::Other(format!("Could not load tipset state: {e:#}")))?;
        let chain_rand = self.chain_rand(ts.clone());

        // Since we're simulating a future message, pretend we're applying it in the
        // "next" tipset
        let epoch = ts.epoch() + 1;
        let genesis_info = GenesisInfo::from_chain_config(self.chain_config().clone());
        // FVM requires a stack size of 64MiB. The alternative is to use `ThreadedExecutor` from
        // FVM, but that introduces some constraints, and possible deadlocks.
        let (ret, duration, state_cid) = stacker::grow(64 << 20, || -> anyhow::Result<_> {
            let mut vm = VM::new(
                ExecutionContext {
                    heaviest_tipset: ts.clone(),
                    state_tree_root: state_root,
                    epoch,
                    rand: Box::new(chain_rand),
                    base_fee: ts.block_headers().first().parent_base_fee.clone(),
                    circ_supply: genesis_info.get_vm_circulating_supply(
                        epoch,
                        self.chain_index().db(),
                        &state_root,
                    )?,
                    chain_config: self.chain_config().shallow_clone(),
                    chain_index: self.chain_index().shallow_clone(),
                    timestamp: ts.min_timestamp(),
                },
                &self.engine,
                VMTrace::NotTraced,
            )?;

            for msg in prior_messages {
                vm.apply_message(msg)?;
            }
            let from_actor = vm
                .get_actor(&message.from())
                .map_err(|e| Error::Other(format!("Could not get actor from state: {e:#}")))?
                .ok_or_else(|| Error::Other("cant find actor in state tree".to_string()))?;

            message.set_sequence(from_actor.sequence);
            let (ret, duration) = vm.apply_message(message)?;
            let state_root = match vm_flush {
                VMFlush::Flush => Some(vm.flush()?),
                VMFlush::Skip => None,
            };
            Ok((ret, duration, state_root))
        })?;

        Ok((
            InvocResult::new(message.message().clone(), &ret),
            ret,
            duration,
            state_cid,
        ))
    }
}

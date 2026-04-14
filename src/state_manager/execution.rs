// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use super::state_computation::{TipsetExecutor, apply_block_messages, validate_tipsets};
use super::utils::structured;
use super::{ExecutedTipset, NO_CALLBACK, StateManager};
use crate::blocks::Tipset;
use crate::chain::index::ResolveNullTipset;
use crate::interpreter::{CalledAt, MessageCallbackCtx, VMTrace};
use crate::rpc::state::{ApiInvocResult, MessageGasCost};
use crate::shim::econ::TokenAmount;
use crate::utils::ShallowClone as _;
use anyhow::{Context as _, bail};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use num_traits::identities::Zero;
use std::ops::RangeInclusive;
use std::sync::Arc;

impl<DB> StateManager<DB>
where
    DB: Blockstore + Send + Sync + 'static,
{
    /// Replays the given message and returns the result of executing the
    /// indicated message, assuming it was executed in the indicated tipset.
    pub async fn replay(self: &Arc<Self>, ts: Tipset, mcid: Cid) -> Result<ApiInvocResult, Error> {
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || this.replay_blocking(ts, mcid)).await?
    }

    /// Blocking version of `replay`
    pub fn replay_blocking(
        self: &Arc<Self>,
        ts: Tipset,
        mcid: Cid,
    ) -> Result<ApiInvocResult, Error> {
        const REPLAY_HALT: &str = "replay_halt";

        let mut api_invoc_result = None;
        let callback = |ctx: MessageCallbackCtx<'_>| {
            match ctx.at {
                CalledAt::Applied | CalledAt::Reward
                    if api_invoc_result.is_none() && ctx.cid == mcid =>
                {
                    api_invoc_result = Some(ApiInvocResult {
                        msg_cid: ctx.message.cid(),
                        msg: ctx.message.message().clone(),
                        msg_rct: Some(ctx.apply_ret.msg_receipt()),
                        error: ctx.apply_ret.failure_info().unwrap_or_default(),
                        duration: ctx.duration.as_nanos().clamp(0, u128::from(u64::MAX)) as u64,
                        gas_cost: MessageGasCost::new(ctx.message.message(), ctx.apply_ret)?,
                        execution_trace: structured::parse_events(ctx.apply_ret.exec_trace())
                            .unwrap_or_default(),
                    });
                    anyhow::bail!(REPLAY_HALT);
                }
                _ => Ok(()),
            }
        };
        let result = self.compute_tipset_state_blocking(ts, Some(callback), VMTrace::Traced);
        if let Err(error_message) = result
            && error_message.to_string() != REPLAY_HALT
        {
            return Err(Error::Other(format!(
                "unexpected error during execution : {error_message:}"
            )));
        }
        api_invoc_result.ok_or_else(|| Error::Other("failed to replay".into()))
    }

    /// Replays a tipset up to a target message, capturing the state root before
    /// and after execution.
    pub async fn replay_for_prestate(
        self: &Arc<Self>,
        ts: Tipset,
        target_message_cid: Cid,
    ) -> Result<(Cid, ApiInvocResult, Cid), Error> {
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            this.replay_for_prestate_blocking(ts, target_message_cid)
        })
        .await
        .map_err(|e| Error::Other(format!("{e}")))?
    }

    fn replay_for_prestate_blocking(
        self: &Arc<Self>,
        ts: Tipset,
        target_msg_cid: Cid,
    ) -> Result<(Cid, ApiInvocResult, Cid), Error> {
        if ts.epoch() == 0 {
            return Err(Error::Other(
                "cannot trace messages in the genesis block".into(),
            ));
        }

        let genesis_timestamp = self.chain_store().genesis_block_header().timestamp;
        let exec = TipsetExecutor::new(
            self.chain_index().shallow_clone(),
            self.chain_config().shallow_clone(),
            self.beacon_schedule().shallow_clone(),
            self.engine(),
            ts.shallow_clone(),
        );
        let mut no_cb = NO_CALLBACK;
        let (parent_state, epoch, block_messages) =
            exec.prepare_parent_state(genesis_timestamp, VMTrace::NotTraced, &mut no_cb)?;

        Ok(stacker::grow(64 << 20, || {
            let mut vm =
                exec.create_vm(parent_state, epoch, ts.min_timestamp(), VMTrace::NotTraced)?;
            let mut processed = ahash::HashSet::default();

            for block in block_messages.iter() {
                let mut penalty = TokenAmount::zero();
                let mut gas_reward = TokenAmount::zero();

                for msg in block.messages.iter() {
                    let cid = msg.cid();
                    if processed.contains(&cid) {
                        continue;
                    }

                    processed.insert(cid);

                    if cid == target_msg_cid {
                        let pre_root = vm.flush()?;
                        let mut traced_vm =
                            exec.create_vm(pre_root, epoch, ts.min_timestamp(), VMTrace::Traced)?;
                        let (ret, duration) = traced_vm.apply_message(msg)?;
                        let post_root = traced_vm.flush()?;

                        return Ok((
                            pre_root,
                            ApiInvocResult {
                                msg_cid: cid,
                                msg: msg.message().clone(),
                                msg_rct: Some(ret.msg_receipt()),
                                error: ret.failure_info().unwrap_or_default(),
                                duration: duration.as_nanos().clamp(0, u128::from(u64::MAX)) as u64,
                                gas_cost: MessageGasCost::default(),
                                execution_trace: structured::parse_events(ret.exec_trace())
                                    .unwrap_or_default(),
                            },
                            post_root,
                        ));
                    }

                    let (ret, _) = vm.apply_message(msg)?;
                    gas_reward += ret.miner_tip();
                    penalty += ret.penalty();
                }

                if let Some(rew_msg) =
                    vm.reward_message(epoch, block.miner, block.win_count, penalty, gas_reward)?
                {
                    let (ret, _) = vm.apply_implicit_message(&rew_msg)?;
                    if let Some(err) = ret.failure_info() {
                        bail!(
                            "failed to apply reward message for miner {}: {err}",
                            block.miner
                        );
                    }

                    if !ret.msg_receipt().exit_code().is_success() {
                        bail!(
                            "reward application message failed (exit: {:?})",
                            ret.msg_receipt().exit_code()
                        );
                    }
                }
            }

            bail!("message {target_msg_cid} not found in tipset")
        })?)
    }

    /// Validates all tipsets at epoch `start..=end` behind the heaviest tipset.
    #[tracing::instrument(skip(self))]
    pub fn validate_range(&self, epochs: RangeInclusive<i64>) -> anyhow::Result<()> {
        let heaviest = self.heaviest_tipset();
        let heaviest_epoch = heaviest.epoch();
        let end = self
            .chain_index()
            .tipset_by_height(*epochs.end(), heaviest, ResolveNullTipset::TakeOlder)
            .with_context(|| {
                format!(
            "couldn't get a tipset at height {} behind heaviest tipset at height {heaviest_epoch}",
            *epochs.end(),
        )
            })?;

        let tipsets = end
            .chain(self.blockstore())
            .take_while(|ts| ts.epoch() >= *epochs.start());

        self.validate_tipsets(tipsets)
    }

    pub fn validate_tipsets<T>(&self, tipsets: T) -> anyhow::Result<()>
    where
        T: Iterator<Item = Tipset> + Send,
    {
        let genesis_timestamp = self.chain_store().genesis_block_header().timestamp;
        validate_tipsets(
            genesis_timestamp,
            self.chain_index(),
            self.chain_config(),
            self.beacon_schedule(),
            self.engine(),
            tipsets,
        )
    }

    pub fn execution_trace(&self, tipset: &Tipset) -> anyhow::Result<(Cid, Vec<ApiInvocResult>)> {
        let mut invoc_trace = vec![];

        let genesis_timestamp = self.chain_store().genesis_block_header().timestamp;

        let callback = |ctx: MessageCallbackCtx<'_>| {
            match ctx.at {
                CalledAt::Applied | CalledAt::Reward => {
                    invoc_trace.push(ApiInvocResult {
                        msg_cid: ctx.message.cid(),
                        msg: ctx.message.message().clone(),
                        msg_rct: Some(ctx.apply_ret.msg_receipt()),
                        error: ctx.apply_ret.failure_info().unwrap_or_default(),
                        duration: ctx.duration.as_nanos().clamp(0, u128::from(u64::MAX)) as u64,
                        gas_cost: MessageGasCost::new(ctx.message.message(), ctx.apply_ret)?,
                        execution_trace: structured::parse_events(ctx.apply_ret.exec_trace())
                            .unwrap_or_default(),
                    });
                    Ok(())
                }
                _ => Ok(()),
            }
        };

        let ExecutedTipset { state_root, .. } = apply_block_messages(
            genesis_timestamp,
            self.chain_index().shallow_clone(),
            self.chain_config().shallow_clone(),
            self.beacon_schedule().shallow_clone(),
            self.engine(),
            tipset.shallow_clone(),
            Some(callback),
            VMTrace::Traced,
        )?;

        Ok((state_root, invoc_trace))
    }
}

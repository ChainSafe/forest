// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::circulating_supply::GenesisInfo;
use super::*;
use crate::db::EthMappingsStore;
use crate::interpreter::{BlockMessages, ExecutionContext, VM, VMTrace};
use crate::shim::message::Message;
use crate::state_migration::run_state_migrations;
use crate::utils::ShallowClone as _;
use anyhow::{Context as _, bail, ensure};
use fil_actors_shared::fvm_ipld_amt::{Amt, Amtv0};
use itertools::Itertools as _;
use tracing::{error, info, instrument};

impl<DB> StateManager<DB>
where
    DB: Blockstore + EthMappingsStore + Send + Sync + 'static,
{
    /// Load the state of a tipset, including state root, message receipts
    pub async fn load_tipset_state(self: &Arc<Self>, ts: &Tipset) -> anyhow::Result<TipsetState> {
        if let Some(state) = self.cache.get_map(ts.key(), |et| et.into()) {
            Ok(state)
        } else {
            match self.chain_store().load_child_tipset(ts)? {
                Some(receipt_ts) => Ok(TipsetState {
                    state_root: *receipt_ts.parent_state(),
                    receipt_root: *receipt_ts.parent_message_receipts(),
                }),
                None => Ok(self.load_executed_tipset(ts).await?.into()),
            }
        }
    }

    /// Load an executed tipset, including state root, message receipts and events with caching.
    pub async fn load_executed_tipset(
        self: &Arc<Self>,
        ts: &Tipset,
    ) -> anyhow::Result<ExecutedTipset> {
        // validate the existence of state trees for post-chain-head-epoch tipsets in case chain head is reset(e.g. manually or via GC).
        if ts.epoch() >= self.heaviest_tipset().epoch()
            && let Some(cached) = self.cache.get(ts.key())
        {
            if StateTree::new_from_root(self.blockstore_owned(), &cached.state_root).is_ok() {
                return Ok(cached);
            } else {
                self.cache.remove(ts.key());
            }
        }
        self.cache
            .get_or_else(ts.key(), || async move {
                let receipt_ts = self.chain_store().load_child_tipset(ts)?;
                self.load_executed_tipset_inner(ts, receipt_ts.as_ref())
                    .await
            })
            .await
    }

    async fn load_executed_tipset_inner(
        self: &Arc<Self>,
        msg_ts: &Tipset,
        // when `msg_ts` is the current head, `receipt_ts` is `None`
        receipt_ts: Option<&Tipset>,
    ) -> anyhow::Result<ExecutedTipset> {
        if let Some(receipt_ts) = receipt_ts {
            anyhow::ensure!(
                msg_ts.key() == receipt_ts.parents(),
                "message tipset should be the parent of message receipt tipset"
            );
        }
        let mut recomputed = false;
        let (state_root, receipt_root, receipts) = match receipt_ts.and_then(|ts| {
            let receipt_root = *ts.parent_message_receipts();
            Receipt::get_receipts(self.cs.blockstore(), receipt_root)
                .ok()
                .map(|r| (*ts.parent_state(), receipt_root, r))
        }) {
            Some((state_root, receipt_root, receipts)) => (state_root, receipt_root, receipts),
            None => {
                let state_output = self
                    .compute_tipset_state(msg_ts.shallow_clone(), NO_CALLBACK, VMTrace::NotTraced)
                    .await?;
                recomputed = true;
                (
                    state_output.state_root,
                    state_output.receipt_root,
                    Receipt::get_receipts(self.cs.blockstore(), state_output.receipt_root)?,
                )
            }
        };

        let messages = self.chain_store().messages_for_tipset(msg_ts)?;
        anyhow::ensure!(
            messages.len() == receipts.len(),
            "mismatching message and receipt counts ({} messages, {} receipts)",
            messages.len(),
            receipts.len()
        );
        let mut executed_messages = Vec::with_capacity(messages.len());
        for (message, receipt) in messages.iter().cloned().zip(receipts) {
            let events = if let Some(events_root) = receipt.events_root() {
                Some(
                    match StampedEvent::get_events(self.cs.blockstore(), &events_root) {
                        Ok(events) => events,
                        Err(e) if recomputed => return Err(e),
                        Err(_) => {
                            self.compute_tipset_state(
                                msg_ts.shallow_clone(),
                                NO_CALLBACK,
                                VMTrace::NotTraced,
                            )
                            .await?;
                            recomputed = true;
                            StampedEvent::get_events(self.cs.blockstore(), &events_root)?
                        }
                    },
                )
            } else {
                None
            };
            executed_messages.push(ExecutedMessage {
                message,
                receipt,
                events,
            });
        }
        Ok(ExecutedTipset {
            state_root,
            receipt_root,
            executed_messages: Arc::new(executed_messages),
        })
    }

    /// Conceptually, a [`Tipset`] consists of _blocks_ which share an _epoch_.
    /// Each _block_ contains _messages_, which are executed by the _Filecoin Virtual Machine_.
    ///
    /// VM message execution essentially looks like this:
    /// ```text
    /// state[N-900..N] * message = state[N+1]
    /// ```
    ///
    /// The `state`s above are stored in the `IPLD Blockstore`, and can be referred to by
    /// a [`Cid`] - the _state root_.
    /// The previous 900 states (configurable, see
    /// <https://docs.filecoin.io/reference/general/glossary/#finality>) can be
    /// queried when executing a message, so a store needs at least that many.
    /// (a snapshot typically contains 2000, for example).
    ///
    /// Each message costs FIL to execute - this is _gas_.
    /// After execution, the message has a _receipt_, showing how much gas was spent.
    /// This is similarly a [`Cid`] into the block store.
    ///
    /// For details, see the documentation for [`apply_block_messages`].
    ///
    pub async fn compute_tipset_state(
        self: &Arc<Self>,
        tipset: Tipset,
        callback: Option<impl FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()> + Send + 'static>,
        enable_tracing: VMTrace,
    ) -> Result<ExecutedTipset, Error> {
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            this.compute_tipset_state_blocking(tipset, callback, enable_tracing)
        })
        .await?
    }

    /// Blocking version of `compute_tipset_state`
    pub fn compute_tipset_state_blocking(
        &self,
        tipset: Tipset,
        callback: Option<impl FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()>>,
        enable_tracing: VMTrace,
    ) -> Result<ExecutedTipset, Error> {
        let epoch = tipset.epoch();
        let has_callback = callback.is_some();
        info!(
            "Evaluating tipset: EPOCH={epoch}, blocks={}, tsk={}",
            tipset.len(),
            tipset.key(),
        );
        Ok(apply_block_messages(
            self.chain_store().genesis_block_header().timestamp,
            self.chain_index().shallow_clone(),
            self.chain_config().shallow_clone(),
            self.beacon_schedule().shallow_clone(),
            &self.engine,
            tipset,
            callback,
            enable_tracing,
        )
        .map_err(|e| {
            if has_callback {
                e
            } else {
                e.context(format!("Failed to compute tipset state@{epoch}"))
            }
        })?)
    }

    #[instrument(skip_all)]
    pub async fn compute_state(
        self: &Arc<Self>,
        height: ChainEpoch,
        messages: Vec<Message>,
        tipset: Tipset,
        callback: Option<impl FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()> + Send + 'static>,
        enable_tracing: VMTrace,
    ) -> Result<ExecutedTipset, Error> {
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            this.compute_state_blocking(height, messages, tipset, callback, enable_tracing)
        })
        .await?
    }

    /// Blocking version of `compute_state`
    #[tracing::instrument(skip_all)]
    pub fn compute_state_blocking(
        &self,
        height: ChainEpoch,
        messages: Vec<Message>,
        tipset: Tipset,
        callback: Option<impl FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()>>,
        enable_tracing: VMTrace,
    ) -> Result<ExecutedTipset, Error> {
        Ok(compute_state(
            height,
            messages,
            tipset,
            self.chain_store().genesis_block_header().timestamp,
            self.chain_index().shallow_clone(),
            self.chain_config().shallow_clone(),
            self.beacon_schedule().shallow_clone(),
            &self.engine,
            callback,
            enable_tracing,
        )?)
    }
}

pub fn validate_tipsets<DB, T>(
    genesis_timestamp: u64,
    chain_index: &ChainIndex<DB>,
    chain_config: &Arc<ChainConfig>,
    beacon: &Arc<BeaconSchedule>,
    engine: &MultiEngine,
    tipsets: T,
) -> anyhow::Result<()>
where
    DB: Blockstore + EthMappingsStore + Send + Sync + 'static,
    T: Iterator<Item = Tipset> + Send,
{
    // Validate one tipset at a time. Parallelizing the outer loop across tipsets
    // might wedge the global rayon pool.
    // Sequential outer iteration leaves the entire rayon pool free for that
    // already-rich inner parallelism.
    for (child, parent) in tipsets.tuple_windows() {
        info!(height = parent.epoch(), "compute parent state");
        let ExecutedTipset {
            state_root: actual_state,
            receipt_root: actual_receipt,
            ..
        } = apply_block_messages(
            genesis_timestamp,
            chain_index.shallow_clone(),
            chain_config.shallow_clone(),
            beacon.shallow_clone(),
            engine,
            parent,
            NO_CALLBACK,
            VMTrace::NotTraced,
        )
        .context("couldn't compute tipset state")?;
        let expected_receipt = child.min_ticket_block().message_receipts;
        let expected_state = child.parent_state();
        if (expected_state, expected_receipt) != (&actual_state, actual_receipt) {
            error!(
                height = child.epoch(),
                ?expected_state,
                ?expected_receipt,
                ?actual_state,
                ?actual_receipt,
                "state mismatch"
            );
            bail!("state mismatch");
        }
    }
    Ok(())
}

/// Shared context for creating VMs and preparing tipset state.
///
/// Encapsulates randomness source, genesis info, VM construction,
/// null-epoch cron handling, and state migrations.
pub(in crate::state_manager) struct TipsetExecutor<'a, DB: Blockstore + Send + Sync + 'static> {
    tipset: Tipset,
    rand: ChainRand<DB>,
    chain_config: Arc<ChainConfig>,
    chain_index: ChainIndex<DB>,
    genesis_info: GenesisInfo,
    engine: &'a MultiEngine,
}

impl<'a, DB: Blockstore + Send + Sync + 'static> TipsetExecutor<'a, DB> {
    pub(in crate::state_manager) fn new(
        chain_index: ChainIndex<DB>,
        chain_config: Arc<ChainConfig>,
        beacon: Arc<BeaconSchedule>,
        engine: &'a MultiEngine,
        tipset: Tipset,
    ) -> Self {
        let rand = ChainRand::new(
            chain_config.shallow_clone(),
            tipset.shallow_clone(),
            chain_index.shallow_clone(),
            beacon,
        );
        let genesis_info = GenesisInfo::from_chain_config(chain_config.shallow_clone());
        Self {
            tipset,
            rand,
            chain_config,
            chain_index,
            genesis_info,
            engine,
        }
    }

    pub(in crate::state_manager) fn create_vm(
        &self,
        state_root: Cid,
        epoch: ChainEpoch,
        timestamp: u64,
        trace: VMTrace,
    ) -> anyhow::Result<VM<DB>>
    where
        DB: EthMappingsStore,
    {
        let circ_supply = self.genesis_info.get_vm_circulating_supply(
            epoch,
            self.chain_index.db(),
            &state_root,
        )?;
        VM::new(
            ExecutionContext {
                heaviest_tipset: self.tipset.shallow_clone(),
                state_tree_root: state_root,
                epoch,
                rand: Box::new(self.rand.shallow_clone()),
                base_fee: self.tipset.min_ticket_block().parent_base_fee.clone(),
                circ_supply,
                chain_config: self.chain_config.shallow_clone(),
                chain_index: self.chain_index.shallow_clone(),
                timestamp,
            },
            self.engine,
            trace,
        )
    }

    /// Produces the state root ready for message execution by running
    /// null-epoch `crons` and any pending state migrations.
    pub(in crate::state_manager) fn prepare_parent_state<F>(
        &self,
        genesis_timestamp: u64,
        null_epoch_trace: VMTrace,
        cron_callback: &mut Option<F>,
    ) -> anyhow::Result<(Cid, ChainEpoch, Vec<BlockMessages>)>
    where
        DB: EthMappingsStore,
        F: FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()>,
    {
        use crate::shim::clock::EPOCH_DURATION_SECONDS;

        let mut parent_state = *self.tipset.parent_state();
        let parent_epoch = self
            .chain_index
            .load_required_tipset(self.tipset.parents())?
            .epoch();
        let epoch = self.tipset.epoch();

        for epoch_i in parent_epoch..epoch {
            if epoch_i > parent_epoch {
                let timestamp = genesis_timestamp + ((EPOCH_DURATION_SECONDS * epoch_i) as u64);
                parent_state = stacker::grow(64 << 20, || -> anyhow::Result<Cid> {
                    let mut vm =
                        self.create_vm(parent_state, epoch_i, timestamp, null_epoch_trace)?;
                    if let Err(e) = vm.run_cron(epoch_i, cron_callback.as_mut()) {
                        error!("Beginning of epoch cron failed to run: {e:#}");
                        return Err(e);
                    }
                    vm.flush()
                })?;
            }
            if let Some(new_state) = run_state_migrations(
                epoch_i,
                &self.chain_config,
                self.chain_index.db(),
                &parent_state,
            )? {
                parent_state = new_state;
            }
        }

        let block_messages = BlockMessages::for_tipset(self.chain_index.db(), &self.tipset)?;
        Ok((parent_state, epoch, block_messages))
    }
}

/// Messages are transactions that produce new states. The state (usually
/// referred to as the 'state-tree') is a mapping from actor addresses to actor
/// states. Each block contains the hash of the state-tree that should be used
/// as the starting state when executing the block messages.
///
/// # Execution environment
///
/// Transaction execution has the following inputs:
/// - a current state-tree (stored as IPLD in a key-value database). This
///   reference is in [`Tipset::parent_state`].
/// - up to 900 past state-trees. See
///   <https://docs.filecoin.io/reference/general/glossary/#finality>.
/// - up to 900 past tipset IDs.
/// - a deterministic source of randomness.
/// - the circulating supply of FIL (see
///   <https://filecoin.io/blog/filecoin-circulating-supply/>). The circulating
///   supply is determined by the epoch and the states of a few key actors.
/// - the base fee (see <https://spec.filecoin.io/systems/filecoin_vm/gas_fee/>).
///   This value is defined by `tipset.parent_base_fee`.
/// - the genesis timestamp (UNIX epoch time when the first block was
///   mined/created).
/// - a chain configuration (maps epoch to network version, has chain specific
///   settings).
///
/// The result of running a set of block messages is an index to the final
/// state-tree and an index to an array of message receipts (listing gas used,
/// return codes, etc).
///
/// # Cron and null tipsets
///
/// Once per epoch, after all messages have run, a special 'cron' transaction
/// must be executed. The tasks of the 'cron' transaction include running batch
/// jobs and keeping the state up-to-date with the current epoch.
///
/// It can happen that no blocks are mined in an epoch. The tipset for such an
/// epoch is called a null tipset. A null tipset has no identity and cannot be
/// directly executed. This is a problem for 'cron' which must run for every
/// epoch, even if there are no messages. The fix is to run 'cron' if there are
/// any null tipsets between the current epoch and the parent epoch.
///
/// Imagine the blockchain looks like this with a null tipset at epoch 9:
///
/// ```text
/// ┌────────┐ ┌────┐ ┌───────┐  ┌───────┐
/// │Epoch 10│ │Null│ │Epoch 8├──►Epoch 7├─►
/// └───┬────┘ └────┘ └───▲───┘  └───────┘
///     └─────────────────┘
/// ```
///
/// The parent of tipset-epoch-10 is tipset-epoch-8. Before executing the
/// messages in epoch 10, we have to run cron for epoch 9. However, running
/// 'cron' requires the timestamp of the youngest block in the tipset (which
/// doesn't exist because there are no blocks in the tipset). Lotus dictates that
/// the timestamp of a null tipset is `30s * epoch` after the genesis timestamp.
/// So, in the above example, if the genesis block was mined at time `X`, the
/// null tipset for epoch 9 will have timestamp `X + 30 * 9`.
///
/// # Migrations
///
/// Migrations happen between network upgrades and modify the state tree. If a
/// migration is scheduled for epoch 10, it will be run _after_ the messages for
/// epoch 10. The tipset for epoch 11 will link the state-tree produced by the
/// migration.
///
/// Example timeline with a migration at epoch 10:
///   1. Tipset-epoch-10 executes, producing state-tree A.
///   2. Migration consumes state-tree A and produces state-tree B.
///   3. Tipset-epoch-11 executes, consuming state-tree B (rather than A).
///
/// Note: The migration actually happens when tipset-epoch-11 executes. This is
///       because tipset-epoch-10 may be null and therefore not executed at all.
///
/// # Caching
///
/// Scanning the blockchain to find past tipsets and state-trees may be slow.
/// The `ChainStore` caches recent tipsets to make these scans faster.
#[allow(clippy::too_many_arguments)]
pub fn apply_block_messages<DB>(
    genesis_timestamp: u64,
    chain_index: ChainIndex<DB>,
    chain_config: Arc<ChainConfig>,
    beacon: Arc<BeaconSchedule>,
    engine: &MultiEngine,
    tipset: Tipset,
    mut callback: Option<impl FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()>>,
    enable_tracing: VMTrace,
) -> anyhow::Result<ExecutedTipset>
where
    DB: Blockstore + EthMappingsStore + Send + Sync + 'static,
{
    // This function will:
    // 1. handle the genesis block as a special case
    // 2. run 'cron' for any null-tipsets between the current tipset and our parent tipset
    // 3. run migrations
    // 4. execute block messages
    // 5. write the state-tree to the DB and return the CID

    // step 1: special case for genesis block
    if tipset.epoch() == 0 {
        // NB: This is here because the process that executes blocks requires that the
        // block miner reference a valid miner in the state tree. Unless we create some
        // magical genesis miner, this won't work properly, so we short circuit here
        // This avoids the question of 'who gets paid the genesis block reward'
        let message_receipts = tipset.min_ticket_block().message_receipts;
        return Ok(ExecutedTipset {
            state_root: *tipset.parent_state(),
            receipt_root: message_receipts,
            executed_messages: vec![].into(),
        });
    }

    let exec = TipsetExecutor::new(
        chain_index.shallow_clone(),
        chain_config,
        beacon,
        engine,
        tipset.shallow_clone(),
    );

    // step 2: running cron for any null-tipsets
    // step 3: run migrations
    let (parent_state, epoch, block_messages) =
        exec.prepare_parent_state(genesis_timestamp, enable_tracing, &mut callback)?;

    // FVM requires a stack size of 64MiB. The alternative is to use `ThreadedExecutor` from
    // FVM, but that introduces some constraints, and possible deadlocks.
    stacker::grow(64 << 20, || -> anyhow::Result<ExecutedTipset> {
        let mut vm = exec.create_vm(parent_state, epoch, tipset.min_timestamp(), enable_tracing)?;

        // step 4: apply tipset messages
        let (receipts, events, events_roots) =
            vm.apply_block_messages(&block_messages, epoch, callback)?;

        // step 5: construct receipt root from receipts
        let receipt_root = Amtv0::new_from_iter(chain_index.db(), receipts.iter())?;

        // step 6: store events AMTs in the blockstore
        for (events, events_root) in events.iter().zip(events_roots.iter()) {
            if let Some(events) = events {
                let event_root =
                    events_root.context("events root should be present when events present")?;
                // Store the events AMT - the root CID should match the one computed by FVM
                let derived_event_root = Amt::new_from_iter_with_bit_width(
                    chain_index.db(),
                    EVENTS_AMT_BITWIDTH,
                    events.iter(),
                )
                .map_err(|e| Error::Other(format!("failed to store events AMT: {e}")))?;

                // Verify the stored root matches the FVM-computed root
                ensure!(
                    derived_event_root == event_root,
                    "Events AMT root mismatch: derived={derived_event_root}, actual={event_root}."
                );
            }
        }

        let state_root = vm.flush()?;

        // Update executed tipset cache
        let messages: Vec<ChainMessage> = block_messages
            .into_iter()
            .flat_map(|bm| bm.messages)
            .collect_vec();
        anyhow::ensure!(
            messages.len() == receipts.len() && messages.len() == events.len(),
            "length of messages, receipts, and events should match",
        );
        Ok(ExecutedTipset {
            state_root,
            receipt_root,
            executed_messages: messages
                .into_iter()
                .zip(receipts)
                .zip(events)
                .map(|((message, receipt), events)| ExecutedMessage {
                    message,
                    receipt,
                    events,
                })
                .collect_vec()
                .into(),
        })
    })
}

#[allow(clippy::too_many_arguments)]
pub(in crate::state_manager) fn compute_state<DB>(
    _height: ChainEpoch,
    messages: Vec<Message>,
    tipset: Tipset,
    genesis_timestamp: u64,
    chain_index: ChainIndex<DB>,
    chain_config: Arc<ChainConfig>,
    beacon: Arc<BeaconSchedule>,
    engine: &MultiEngine,
    callback: Option<impl FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()>>,
    enable_tracing: VMTrace,
) -> anyhow::Result<ExecutedTipset>
where
    DB: Blockstore + EthMappingsStore + Send + Sync + 'static,
{
    if !messages.is_empty() {
        anyhow::bail!("Applying messages is not yet implemented.");
    }

    let output = apply_block_messages(
        genesis_timestamp,
        chain_index,
        chain_config,
        beacon,
        engine,
        tipset,
        callback,
        enable_tracing,
    )?;

    Ok(output)
}

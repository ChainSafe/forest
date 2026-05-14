// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::TipsetKey;
use crate::message::MessageRead as _;
use ahash::{HashMap, HashMapExt as _};
use anyhow::Context as _;
use futures::{FutureExt, channel::oneshot, select};
use tokio::sync::{RwLock, broadcast::error::RecvError};
use tracing::warn;

impl StateManager {
    /// Check if tipset had executed the message, by loading the receipt based
    /// on the index of the message in the block.
    fn tipset_executed_message(
        &self,
        tipset: &Tipset,
        message: &ChainMessage,
        allow_replaced: bool,
    ) -> Result<Option<Receipt>, Error> {
        if tipset.epoch() == 0 {
            return Ok(None);
        }
        let message_from_address = message.from();
        let message_sequence = message.sequence();
        // Load parent state.
        let pts = self
            .chain_index()
            .load_required_tipset(tipset.parents())
            .map_err(|err| Error::Other(format!("Failed to load tipset: {err}")))?;
        let messages = self
            .cs
            .messages_for_tipset(&pts)
            .map_err(|err| Error::Other(format!("Failed to load messages for tipset: {err}")))?;
        messages
            .iter()
            .enumerate()
            // iterate in reverse because we going backwards through the chain
            .rev()
            .filter(|(_, s)| {
                s.sequence() == message_sequence
                    && s.from() == message_from_address
                    && s.equal_call(message)
            })
            .map(|(index, m)| {
                // A replacing message is a message with a different CID,
                // any of Gas values, and different signature, but with all
                // other parameters matching (source/destination, nonce, params, etc.)
                if !allow_replaced && message.cid() != m.cid(){
                    Err(Error::Other(format!(
                        "found message with equal nonce and call params but different CID. wanted {}, found: {}, nonce: {}, from: {}",
                        message.cid(),
                        m.cid(),
                        message.sequence(),
                        message.from(),
                    )))
                } else {
                    let block_header = tipset.block_headers().first();
                    crate::chain::get_parent_receipt(
                        self.db(),
                        block_header,
                        index,
                    )
                        .map_err(|err| Error::Other(format!("Failed to get parent receipt (message_receipts={}, index={index}, error={err})", block_header.message_receipts)))
                }
            })
            .next()
            .unwrap_or(Ok(None))
    }

    fn check_search(
        &self,
        mut current: Tipset,
        message: &ChainMessage,
        lookback_max_epoch: ChainEpoch,
        allow_replaced: bool,
    ) -> Result<Option<(Tipset, Receipt)>, Error> {
        let message_from_address = message.from();
        let message_sequence = message.sequence();
        let mut current_actor_state = self
            .get_required_actor(&message_from_address, *current.parent_state())
            .map_err(Error::state)?;
        let message_from_id = self.lookup_required_id(&message_from_address, &current)?;

        while current.epoch() >= lookback_max_epoch {
            let parent_tipset = self
                .chain_index()
                .load_required_tipset(current.parents())
                .map_err(|err| {
                    Error::Other(format!(
                        "failed to load tipset during msg wait searchback: {err:}"
                    ))
                })?;

            let parent_actor_state = self
                .get_actor(&message_from_id, *parent_tipset.parent_state())
                .map_err(|e| Error::State(e.to_string()))?;

            if parent_actor_state.is_none()
                || (current_actor_state.sequence > message_sequence
                    && parent_actor_state.as_ref().unwrap().sequence <= message_sequence)
            {
                let receipt = self
                    .tipset_executed_message(&current, message, allow_replaced)?
                    .context("Failed to get receipt with tipset_executed_message")?;
                return Ok(Some((current, receipt)));
            }

            if let Some(parent_actor_state) = parent_actor_state {
                current = parent_tipset;
                current_actor_state = parent_actor_state;
            } else {
                break;
            }
        }

        Ok(None)
    }

    /// Searches backwards through the chain for a message receipt.
    fn search_back_for_message(
        &self,
        current: Tipset,
        message: &ChainMessage,
        look_back_limit: Option<i64>,
        allow_replaced: Option<bool>,
    ) -> Result<Option<(Tipset, Receipt)>, Error> {
        let current_epoch = current.epoch();
        let allow_replaced = allow_replaced.unwrap_or(true);

        // Calculate the max lookback epoch (inclusive lower bound) for the search.
        let lookback_max_epoch = match look_back_limit {
            // No search: limit = 0 means search 0 epochs
            Some(0) => return Ok(None),
            // Limited search: calculate the inclusive lower bound, clamped to genesis
            // Example: limit=5 at epoch=1000 → min_epoch=996, searches [996,1000] = 5 epochs
            // Example: limit=2000 at epoch=1000 → min_epoch=0, searches [0,1000] = 1001 epochs (all available)
            Some(limit) if limit > 0 => (current_epoch - limit + 1).max(0),
            // Search all the way to genesis (epoch 0)
            _ => 0,
        };

        self.check_search(current, message, lookback_max_epoch, allow_replaced)
    }

    /// Returns a message receipt from a given tipset and message CID.
    pub fn get_receipt(&self, tipset: Tipset, msg: Cid) -> Result<Receipt, Error> {
        let m = crate::chain::get_chain_message(self.db(), &msg)
            .map_err(|e| Error::Other(e.to_string()))?;
        let message_receipt = self.tipset_executed_message(&tipset, &m, true)?;
        if let Some(receipt) = message_receipt {
            return Ok(receipt);
        }

        let maybe_tuple = self.search_back_for_message(tipset, &m, None, None)?;
        let message_receipt = maybe_tuple
            .ok_or_else(|| {
                Error::Other("Could not get receipt from search back message".to_string())
            })?
            .1;
        Ok(message_receipt)
    }

    /// `WaitForMessage` blocks until a message appears on chain. It looks
    /// backwards in the chain to see if this has already happened. It
    /// guarantees that the message has been on chain for at least
    /// confidence epochs without being reverted before returning.
    pub async fn wait_for_message(
        &self,
        msg_cid: Cid,
        confidence: i64,
        look_back_limit: Option<ChainEpoch>,
        allow_replaced: Option<bool>,
    ) -> Result<(Option<Tipset>, Option<Receipt>), Error> {
        let mut head_changes_rx = self.cs.subscribe_head_changes();
        let (sender, mut receiver) = oneshot::channel::<()>();
        let message = crate::chain::get_chain_message(self.db(), &msg_cid)
            .map_err(|err| Error::Other(format!("failed to load message {err:}")))?;
        let current_tipset = self.heaviest_tipset();
        let maybe_message_receipt =
            self.tipset_executed_message(&current_tipset, &message, true)?;
        if let Some(r) = maybe_message_receipt {
            return Ok((Some(current_tipset.shallow_clone()), Some(r)));
        }

        let mut candidate_tipset: Option<Tipset> = None;
        let mut candidate_receipt: Option<Receipt> = None;

        let sm_cloned = self.shallow_clone();

        let message_for_task = message.clone();
        let height_of_head = current_tipset.epoch();
        let task = tokio::task::spawn(async move {
            let back_tuple = sm_cloned.search_back_for_message(
                current_tipset,
                &message_for_task,
                look_back_limit,
                allow_replaced,
            )?;
            sender
                .send(())
                .map_err(|e| Error::Other(format!("Could not send to channel {e:?}")))?;
            Ok::<_, Error>(back_tuple)
        });

        let reverts: Arc<RwLock<HashMap<TipsetKey, bool>>> = Arc::new(RwLock::new(HashMap::new()));
        let block_revert = reverts.clone();
        let sm_cloned = self.shallow_clone();

        // Wait for message to be included in head change.
        let mut subscriber_poll = tokio::task::spawn(async move {
            loop {
                match head_changes_rx.recv().await {
                    Ok(head_changes) => {
                        for tipset in head_changes.reverts {
                            if candidate_tipset
                                .as_ref()
                                .is_some_and(|candidate| candidate.key() == tipset.key())
                            {
                                candidate_tipset = None;
                                candidate_receipt = None;
                            }
                        }
                        for tipset in head_changes.applies {
                            if candidate_tipset
                                .as_ref()
                                .map(|s| tipset.epoch() >= s.epoch() + confidence)
                                .unwrap_or_default()
                            {
                                return Ok((candidate_tipset, candidate_receipt));
                            }
                            let poll_receiver = receiver.try_recv();
                            if let Ok(Some(_)) = poll_receiver {
                                block_revert
                                    .write()
                                    .await
                                    .insert(tipset.key().to_owned(), true);
                            }

                            let maybe_receipt =
                                sm_cloned.tipset_executed_message(&tipset, &message, true)?;
                            if let Some(receipt) = maybe_receipt {
                                if confidence == 0 {
                                    return Ok((Some(tipset), Some(receipt)));
                                }
                                candidate_tipset = Some(tipset);
                                candidate_receipt = Some(receipt)
                            }
                        }
                    }
                    Err(RecvError::Lagged(i)) => {
                        warn!(
                            "wait for message head change subscriber lagged, skipped {} events",
                            i
                        );
                    }
                    Err(RecvError::Closed) => break,
                }
            }
            Ok((None, None))
        })
        .fuse();

        // Search backwards for message.
        let mut search_back_poll = tokio::task::spawn(async move {
            let back_tuple = task.await.map_err(|e| {
                Error::Other(format!("Could not search backwards for message {e}"))
            })??;
            if let Some((back_tipset, back_receipt)) = back_tuple {
                let should_revert = *reverts
                    .read()
                    .await
                    .get(back_tipset.key())
                    .unwrap_or(&false);
                let larger_height_of_head = height_of_head >= back_tipset.epoch() + confidence;
                if !should_revert && larger_height_of_head {
                    return Ok::<_, Error>((Some(back_tipset), Some(back_receipt)));
                }
                return Ok((None, None));
            }
            Ok((None, None))
        })
        .fuse();

        // Await on first future to finish.
        loop {
            select! {
                res = subscriber_poll => {
                    return res?
                }
                res = search_back_poll => {
                    if let Ok((Some(ts), Some(rct))) = res? {
                        return Ok((Some(ts), Some(rct)));
                    }
                }
            }
        }
    }

    pub async fn search_for_message(
        &self,
        from: Option<Tipset>,
        msg_cid: Cid,
        look_back_limit: Option<i64>,
        allow_replaced: Option<bool>,
    ) -> Result<Option<(Tipset, Receipt)>, Error> {
        let from = from.unwrap_or_else(|| self.heaviest_tipset());
        let message = crate::chain::get_chain_message(self.db(), &msg_cid)
            .map_err(|err| Error::Other(format!("failed to load message {err}")))?;
        let current_tipset = self.heaviest_tipset();
        let maybe_message_receipt =
            self.tipset_executed_message(&from, &message, allow_replaced.unwrap_or(true))?;
        if let Some(r) = maybe_message_receipt {
            Ok(Some((from, r)))
        } else {
            self.search_back_for_message(current_tipset, &message, look_back_limit, allow_replaced)
        }
    }
}

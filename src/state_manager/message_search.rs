// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::TipsetKey;
use crate::message::MessageRead as _;
use ahash::HashSet;
use parking_lot::RwLock;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;
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

    fn check_search_blocking(
        &self,
        mut current: Tipset,
        message: &ChainMessage,
        lookback_max_epoch: ChainEpoch,
        allow_replaced: bool,
        cancellation_token: &CancellationToken,
    ) -> Result<Option<(Tipset, Receipt)>, Error> {
        let message_from_address = message.from();
        let message_sequence = message.sequence();
        let mut current_actor_state = self
            .get_required_actor(&message_from_address, *current.parent_state())
            .map_err(Error::state)?;
        let message_from_id = self.lookup_required_id(&message_from_address, &current)?;

        while !cancellation_token.is_cancelled() && current.epoch() >= lookback_max_epoch {
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

            if (parent_actor_state.is_none()
                || (current_actor_state.sequence > message_sequence
                    && parent_actor_state.as_ref().unwrap().sequence <= message_sequence))
                && let Some(receipt) =
                    self.tipset_executed_message(&current, message, allow_replaced)?
            {
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
    fn search_back_for_message_blocking(
        &self,
        current: Tipset,
        message: &ChainMessage,
        look_back_limit: Option<i64>,
        allow_replaced: Option<bool>,
        cancellation_token: &CancellationToken,
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

        self.check_search_blocking(
            current,
            message,
            lookback_max_epoch,
            allow_replaced,
            cancellation_token,
        )
    }

    /// Returns a message receipt from a given tipset and message CID.
    pub fn get_receipt_blocking(
        &self,
        tipset: Tipset,
        msg: Cid,
        cancellation_token: &CancellationToken,
    ) -> Result<Receipt, Error> {
        let m = crate::chain::get_chain_message(self.db(), &msg)
            .map_err(|e| Error::Other(e.to_string()))?;
        let message_receipt = self.tipset_executed_message(&tipset, &m, true)?;
        if let Some(receipt) = message_receipt {
            return Ok(receipt);
        }

        let maybe_tuple =
            self.search_back_for_message_blocking(tipset, &m, None, None, cancellation_token)?;
        let message_receipt = maybe_tuple
            .ok_or_else(|| {
                Error::Other("Could not get receipt from search back message".to_string())
            })?
            .1;
        Ok(message_receipt)
    }

    pub async fn wait_for_message_with_timeout(
        &self,
        msg_cid: Cid,
        confidence: i64,
        look_back_limit: Option<ChainEpoch>,
        allow_replaced: Option<bool>,
        timeout: Duration,
    ) -> Result<(Tipset, Receipt), Error> {
        let cancellation_token = CancellationToken::new();
        let _cancellation_token_drop_guard = cancellation_token.drop_guard_ref();
        tokio::time::timeout(
            timeout,
            self.wait_for_message(
                msg_cid,
                confidence,
                look_back_limit,
                allow_replaced,
                &cancellation_token,
            ),
        )
        .await
        .map_err(|_| {
            Error::other(format!(
                "wait_for_message timed out after {}",
                humantime::format_duration(timeout)
            ))
        })?
    }

    /// `WaitForMessage` blocks until a message appears on chain. It looks
    /// backwards in the chain to see if this has already happened. It
    /// guarantees that the message has been on chain for at least
    /// confidence epochs without being reverted before returning.
    /// Returns an error when cancelled.
    pub async fn wait_for_message(
        &self,
        msg_cid: Cid,
        confidence: i64,
        look_back_limit: Option<ChainEpoch>,
        allow_replaced: Option<bool>,
        cancellation_token: &CancellationToken,
    ) -> Result<(Tipset, Receipt), Error> {
        let message = Arc::new(
            crate::chain::get_chain_message(self.db(), &msg_cid)
                .map_err(|err| Error::Other(format!("failed to load message {err:}")))?,
        );
        let current_ts = self.heaviest_tipset();
        let maybe_message_receipt = self.tipset_executed_message(&current_ts, &message, true)?;
        if let Some(receipt) = maybe_message_receipt {
            return Ok((current_ts, receipt));
        }

        // For immediate search back response
        let (search_back_tx, search_back_rx) = flume::bounded(1);
        let search_back_candidate: Arc<OnceLock<(Tipset, Receipt)>> = Default::default();
        let reverted: Arc<RwLock<HashSet<TipsetKey>>> = Arc::new(RwLock::new(HashSet::default()));
        // Search back task
        tokio::task::spawn_blocking({
            let sm = self.shallow_clone();
            let message = message.shallow_clone();
            // Cloning tx to avoid all senders being dropped to make `search_back_rx.recv_async()` wait
            let search_back_tx = search_back_tx.clone();
            let search_back_candidate = search_back_candidate.shallow_clone();
            let reverted = reverted.shallow_clone();
            let cancellation_token = cancellation_token.clone();
            move || {
                if let Ok(Some((ts, receipt))) = sm
                    .search_back_for_message_blocking(
                        current_ts,
                        &message,
                        look_back_limit,
                        allow_replaced,
                        &cancellation_token,
                    )
                    .inspect_err(|e| {
                        tracing::warn!("failed to search back for message: {e}");
                    })
                    && !reverted.read().contains(ts.key())
                {
                    if sm.heaviest_tipset().epoch() >= ts.epoch() + confidence {
                        _ = search_back_tx.send((ts, receipt)).inspect_err(|e| {
                            tracing::warn!("failed to send to search_back_tx: {e}");
                        });
                    } else {
                        _ = search_back_candidate.set((ts, receipt)).inspect_err(|_| {
                            tracing::warn!("failed to send to set search_back_candidate");
                        });
                    }
                }
            }
        });

        // Wait for message to be included in head change.
        let subscriber_poll = tokio::task::spawn({
            let cancellation_token = cancellation_token.clone();
            let search_back_candidate = search_back_candidate.shallow_clone();
            let reverted = reverted.shallow_clone();
            let sm = self.shallow_clone();
            async move {
                let mut head_changes_rx = sm.cs.subscribe_head_changes();
                let mut candidate: Option<(Tipset, Receipt)> = None;
                while !cancellation_token.is_cancelled() {
                    match head_changes_rx.recv().await {
                        Ok(head_changes) => {
                            for reverted_ts in head_changes.reverts {
                                reverted.write().insert(reverted_ts.key().clone());

                                if candidate
                                    .as_ref()
                                    .is_some_and(|(ts, _)| ts.key() == reverted_ts.key())
                                {
                                    candidate = None;
                                }
                            }
                            for applied_ts in head_changes.applies {
                                reverted.write().remove(applied_ts.key());

                                // Return if `search_back_candidate` meets confidence requirement
                                if let Some((candidate_ts, candidate_receipt)) =
                                    search_back_candidate.get()
                                    && applied_ts.epoch() >= candidate_ts.epoch() + confidence
                                    && !reverted.read().contains(candidate_ts.key())
                                {
                                    return Ok((
                                        candidate_ts.shallow_clone(),
                                        candidate_receipt.clone(),
                                    ));
                                }

                                // Return if the candidate meets confidence requirement
                                if let Some((candidate_ts, _)) = &candidate
                                    && applied_ts.epoch() >= candidate_ts.epoch() + confidence
                                    && let Some(candidate) = candidate
                                {
                                    return Ok(candidate);
                                }

                                let maybe_receipt =
                                    sm.tipset_executed_message(&applied_ts, &message, true)?;
                                if let Some(receipt) = maybe_receipt {
                                    if confidence == 0 {
                                        // Return if there's no confidence requirement
                                        return Ok((applied_ts, receipt));
                                    } else {
                                        // Otherwise set it as candidate
                                        candidate = Some((applied_ts, receipt));
                                    }
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
                Err(Error::other("cancelled"))
            }
        });

        // Await on first future to finish.
        tokio::select! {
            res = subscriber_poll => {
                res.context("tokio join error")?
            }
            res = search_back_rx.recv_async()  => {
                Ok(res.context("channel receive error")?)
            }
            _ = cancellation_token.cancelled() => {
                Err(Error::other("cancelled"))
            }
        }
    }

    pub async fn search_for_message(
        &self,
        from: Option<Tipset>,
        msg_cid: Cid,
        look_back_limit: Option<i64>,
        allow_replaced: Option<bool>,
        cancellation_token: &CancellationToken,
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
            tokio::task::spawn_blocking({
                let this = self.shallow_clone();
                let cancellation_token = cancellation_token.clone();
                move || {
                    this.search_back_for_message_blocking(
                        current_tipset,
                        &message,
                        look_back_limit,
                        allow_replaced,
                        &cancellation_token,
                    )
                }
            })
            .await?
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{CachingBlockHeader, Chain4U, HeaderBuilder, TxMeta, chain4u};
    use crate::db::MemoryDB;
    use crate::shim::message::Message;
    use crate::shim::state_tree::StateTreeVersion;
    use crate::utils::db::CborStoreExt as _;
    use fil_actors_shared::fvm_ipld_amt::Amtv0 as Amt;

    #[test]
    fn check_search_miss_is_not_an_error() {
        let db = Arc::new(MemoryDB::default());
        let sender = Address::new_id(1000);

        let empty_root = StateTree::new(&db, StateTreeVersion::V5)
            .unwrap()
            .flush()
            .unwrap();
        let mut head_tree = StateTree::new(&db, StateTreeVersion::V5).unwrap();
        head_tree
            .set_actor(
                &sender,
                ActorState::new(
                    Cid::default(),
                    Cid::default(),
                    TokenAmount::default(),
                    0,
                    None,
                ),
            )
            .unwrap();
        let head_root = head_tree.flush().unwrap();

        let empty_amt = Amt::<Cid, _>::new(&db).flush().unwrap();
        let empty_meta = db
            .put_cbor_default(&TxMeta {
                bls_message_root: empty_amt,
                secp_message_root: empty_amt,
            })
            .unwrap();

        let c4u = Chain4U::with_blockstore(db.clone());
        chain4u! {
            in c4u;
            [genesis = HeaderBuilder::new().with_state_root(empty_root).with_timestamp(1).with_messages(empty_meta)]
            -> [head = HeaderBuilder::new().with_state_root(head_root)]
        };

        let cs = ChainStore::new(
            db.clone(),
            Arc::new(ChainConfig::default()),
            CachingBlockHeader::new(genesis.clone()),
        )
        .unwrap();
        let sm = StateManager::new(cs).unwrap();
        let head_ts =
            Tipset::load_required(&db, &TipsetKey::from(nunny::vec![head.cid()])).unwrap();

        let msg: ChainMessage = Message {
            from: sender,
            sequence: 0,
            ..Default::default()
        }
        .into();
        let res = sm.check_search_blocking(head_ts, &msg, 0, true, &CancellationToken::new());

        // The miss must be Ok(None), not an error.
        assert!(matches!(res, Ok(None)), "expected Ok(None), got {res:?}");
    }
}

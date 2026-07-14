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
        let current_actor_state = self
            .get_required_actor(&message_from_address, *current.parent_state())
            .map_err(Error::state)?;
        // The sender's nonce only grows, so once it is at or below the message
        // nonce the message cannot have been executed yet. Walking back would
        // only end at the sender's creation or at pruned state.
        if current_actor_state.sequence <= message_sequence {
            return Ok(None);
        }
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

            match parent_actor_state {
                // The nonce is still above the message nonce at the parent, so
                // the message executed strictly earlier; keep walking back.
                Some(state) if state.sequence > message_sequence => current = parent_tipset,
                // The nonce crossed the message nonce between the parent and
                // `current` (or the sender did not exist yet), so only `current`
                // can have executed the message. No receipt there means a
                // replacing message was executed instead.
                _ => {
                    return Ok(self
                        .tipset_executed_message(&current, message, allow_replaced)?
                        .map(|receipt| (current, receipt)));
                }
            }
        }

        Ok(None)
    }

    /// Searches backwards through the chain for a message receipt.
    fn search_back_for_message_blocking(
        &self,
        current: Tipset,
        message: &ChainMessage,
        look_back_limit: Option<ChainEpoch>,
        allow_replaced: Option<bool>,
        cancellation_token: &CancellationToken,
    ) -> Result<Option<(Tipset, Receipt)>, Error> {
        let current_epoch = current.epoch();
        let allow_replaced = allow_replaced.unwrap_or(true);

        let Some(max_lookback_epoch_inclusive) =
            Self::max_lookback_epoch_inclusive(current_epoch, look_back_limit)
        else {
            return Ok(None);
        };

        self.check_search_blocking(
            current,
            message,
            max_lookback_epoch_inclusive,
            allow_replaced,
            cancellation_token,
        )
    }

    //. Calculates the max lookback epoch (inclusive lower bound) for the search.
    pub fn max_lookback_epoch_inclusive(
        current_epoch: ChainEpoch,
        look_back_limit: Option<ChainEpoch>,
    ) -> Option<ChainEpoch> {
        match look_back_limit {
            // No search: limit = 0 means search 0 epochs
            Some(0) => None,
            // Limited search: calculate the inclusive lower bound, clamped to genesis
            // Example: limit=5 at epoch=1000 → min_epoch=996, searches [996,1000] = 5 epochs
            // Example: limit=2000 at epoch=1000 → min_epoch=0, searches [0,1000] = 1001 epochs (all available)
            Some(limit) if limit > 0 => Some((current_epoch - limit + 1).max(0)),
            // Search all the way to genesis (epoch 0)
            _ => Some(0),
        }
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
        let maybe_message_receipt =
            self.tipset_executed_message(&current_ts, &message, allow_replaced.unwrap_or(true))?;
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

                                let maybe_receipt = sm.tipset_executed_message(
                                    &applied_ts,
                                    &message,
                                    allow_replaced.unwrap_or(true),
                                )?;
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
                        from,
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
    use crate::blocks::{
        CachingBlockHeader, Chain4U, HeaderBuilder, RawBlockHeader, TxMeta, chain4u,
    };
    use crate::chain::ChainStore;
    use crate::db::MemoryDB;
    use crate::networks::ChainConfig;
    use crate::shim::address::Address;
    use crate::shim::econ::TokenAmount;
    use crate::shim::message::Message;
    use crate::shim::state_tree::{ActorState, StateTree, StateTreeVersion};
    use crate::utils::db::CborStoreExt as _;
    use fil_actors_shared::fvm_ipld_amt::Amtv0;
    use fvm_ipld_blockstore::Blockstore;

    const SENDER: Address = Address::new_id(100);

    fn state_root_with_sender_nonce(db: &Arc<MemoryDB>, sequence: u64) -> Cid {
        let mut state_tree = StateTree::new(db, StateTreeVersion::V5).unwrap();
        state_tree
            .set_actor(
                &SENDER,
                ActorState::new(
                    Cid::default(),
                    Cid::default(),
                    TokenAmount::default(),
                    sequence,
                    None,
                ),
            )
            .unwrap();
        state_tree.flush().unwrap()
    }

    fn tx_meta(db: &impl Blockstore, message: Cid) -> Cid {
        let bls_message_root = Amtv0::new_from_iter(db, [message]).unwrap();
        let secp_message_root = Amtv0::new_from_iter(db, std::iter::empty::<Cid>()).unwrap();
        db.put_cbor_default(&TxMeta {
            bls_message_root,
            secp_message_root,
        })
        .unwrap()
    }

    fn receipts_root(db: &impl Blockstore) -> Cid {
        let receipt = fvm_shared4::receipt::Receipt {
            exit_code: fvm_shared4::error::ExitCode::OK,
            return_data: Default::default(),
            gas_used: 0,
            events_root: None,
        };
        Amtv0::new_from_iter(db, [receipt]).unwrap()
    }

    fn message_with_nonce(sequence: u64) -> Message {
        Message {
            from: SENDER,
            to: Address::new_id(101),
            sequence,
            ..Default::default()
        }
    }

    fn state_manager_with_replaced_message_at_head(db: &Arc<MemoryDB>) -> (StateManager, Cid) {
        let message = message_with_nonce(5);
        let msg_cid = db.put_cbor_default(&message).unwrap();
        let replacement = Message {
            gas_limit: 1,
            ..message
        };
        let replacement_cid = db.put_cbor_default(&replacement).unwrap();

        let root_before = state_root_with_sender_nonce(db, 5);
        let root_after = state_root_with_sender_nonce(db, 6);
        let messages = tx_meta(db, replacement_cid);
        let receipts = receipts_root(db);
        let c4u = Chain4U::with_blockstore(db.clone());
        chain4u! {
            in c4u;
            [genesis = HeaderBuilder::new().with_timestamp(7777)]
            -> [_e1 = HeaderBuilder::new().with_state_root(root_before)]
            -> [_e2 = HeaderBuilder::new()
                    .with_state_root(root_before)
                    .with_messages(messages)]
            -> head @ [_e3 = HeaderBuilder::new()
                    .with_state_root(root_after)
                    .with_message_receipts(receipts)]
        };
        (state_manager_with_head(db.clone(), genesis, head), msg_cid)
    }

    fn state_manager_with_head(
        db: Arc<MemoryDB>,
        genesis: &RawBlockHeader,
        head: &Tipset,
    ) -> StateManager {
        let chain_store = ChainStore::new(
            db,
            Arc::new(ChainConfig::default()),
            CachingBlockHeader::new(genesis.clone()),
        )
        .unwrap();
        chain_store.set_heaviest_tipset(head.clone()).unwrap();
        StateManager::new(chain_store).unwrap()
    }

    /// Chain where the sender's nonce is `actor_nonce` at every epoch and the
    /// genesis state is unavailable, like state pruned by GC. Searching must
    /// not walk into the missing state.
    async fn search_pending(
        actor_nonce: u64,
        message_nonce: u64,
    ) -> Result<Option<(Tipset, Receipt)>, Error> {
        let db = Arc::new(MemoryDB::default());
        let root = state_root_with_sender_nonce(&db, actor_nonce);
        let c4u = Chain4U::with_blockstore(db.clone());
        chain4u! {
            in c4u;
            [genesis = HeaderBuilder::new().with_timestamp(7777)]
            -> [_e1 = HeaderBuilder::new().with_state_root(root)]
            -> [_e2 = HeaderBuilder::new().with_state_root(root)]
            -> head @ [_e3 = HeaderBuilder::new().with_state_root(root)]
        };
        let state_manager = state_manager_with_head(db.clone(), genesis, head);

        let message = message_with_nonce(message_nonce);
        let msg_cid = db.put_cbor_default(&message).unwrap();

        state_manager
            .search_for_message(None, msg_cid, None, Some(true), &CancellationToken::new())
            .await
    }

    #[tokio::test]
    async fn search_returns_none_for_message_with_future_nonce() {
        let result = search_pending(5, 10).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn search_returns_none_for_pending_message_at_current_nonce() {
        let result = search_pending(5, 5).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn search_returns_none_for_pending_message_from_fresh_sender() {
        let result = search_pending(0, 0).await.unwrap();
        assert!(result.is_none());
    }

    /// The sender's nonce crossed the message nonce, but a replacing message
    /// with a different call executed instead: the searched message was never
    /// executed, so the result is `None`, not an error.
    #[tokio::test]
    async fn search_returns_none_for_replaced_message() {
        let db = Arc::new(MemoryDB::default());
        let message = message_with_nonce(5);
        let msg_cid = db.put_cbor_default(&message).unwrap();

        let root_before = state_root_with_sender_nonce(&db, 5);
        let root_after = state_root_with_sender_nonce(&db, 6);
        let c4u = Chain4U::with_blockstore(db.clone());
        chain4u! {
            in c4u;
            [genesis = HeaderBuilder::new().with_timestamp(7777)]
            -> [_e1 = HeaderBuilder::new().with_state_root(root_before)]
            -> [_e2 = HeaderBuilder::new().with_state_root(root_before)]
            -> [_e3 = HeaderBuilder::new().with_state_root(root_after)]
            -> head @ [_e4 = HeaderBuilder::new().with_state_root(root_after)]
        };
        let state_manager = state_manager_with_head(db.clone(), genesis, head);

        let result = state_manager
            .search_for_message(None, msg_cid, None, Some(true), &CancellationToken::new())
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn search_finds_executed_message() {
        let db = Arc::new(MemoryDB::default());
        let message = message_with_nonce(5);
        let msg_cid = db.put_cbor_default(&message).unwrap();

        let root_before = state_root_with_sender_nonce(&db, 5);
        let root_after = state_root_with_sender_nonce(&db, 6);
        let messages = tx_meta(&db, msg_cid);
        let receipts = receipts_root(&db);
        let c4u = Chain4U::with_blockstore(db.clone());
        chain4u! {
            in c4u;
            [genesis = HeaderBuilder::new().with_timestamp(7777)]
            -> [_e1 = HeaderBuilder::new().with_state_root(root_before)]
            -> [_e2 = HeaderBuilder::new()
                    .with_state_root(root_before)
                    .with_messages(messages)]
            -> [_e3 = HeaderBuilder::new()
                    .with_state_root(root_after)
                    .with_message_receipts(receipts)]
            -> head @ [_e4 = HeaderBuilder::new().with_state_root(root_after)]
        };
        let state_manager = state_manager_with_head(db.clone(), genesis, head);

        let (tipset, receipt) = state_manager
            .search_for_message(None, msg_cid, None, Some(true), &CancellationToken::new())
            .await
            .unwrap()
            .expect("executed message should be found");
        assert_eq!(tipset.epoch(), 3);
        assert!(receipt.exit_code().is_success());
    }

    /// Searching from an explicit tipset only covers executions at or below
    /// it, like in Lotus, even when the message executed later in the chain.
    #[tokio::test]
    async fn search_from_older_tipset_ignores_later_execution() {
        let db = Arc::new(MemoryDB::default());
        let message = message_with_nonce(5);
        let msg_cid = db.put_cbor_default(&message).unwrap();

        let root_before = state_root_with_sender_nonce(&db, 5);
        let root_after = state_root_with_sender_nonce(&db, 6);
        let messages = tx_meta(&db, msg_cid);
        let receipts = receipts_root(&db);
        let c4u = Chain4U::with_blockstore(db.clone());
        chain4u! {
            in c4u;
            [genesis = HeaderBuilder::new().with_timestamp(7777)]
            -> [_e1 = HeaderBuilder::new().with_state_root(root_before)]
            -> from @ [_e2 = HeaderBuilder::new()
                    .with_state_root(root_before)
                    .with_messages(messages)]
            -> [_e3 = HeaderBuilder::new()
                    .with_state_root(root_after)
                    .with_message_receipts(receipts)]
            -> head @ [_e4 = HeaderBuilder::new().with_state_root(root_after)]
        };
        let state_manager = state_manager_with_head(db.clone(), genesis, head);

        let result = state_manager
            .search_for_message(
                Some(from.clone()),
                msg_cid,
                None,
                Some(true),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(result.is_none());
    }

    /// A replacing message executed on chain. When replacements are
    /// disallowed, `wait_for_message` must forward `allow_replaced = false`
    /// and surface an error instead of silently returning the replacement's
    /// receipt.
    #[tokio::test]
    async fn wait_for_message_rejects_replaced_when_disallowed() {
        let db = Arc::new(MemoryDB::default());
        let (state_manager, msg_cid) = state_manager_with_replaced_message_at_head(&db);

        let result = state_manager
            .wait_for_message(msg_cid, 0, None, Some(false), &CancellationToken::new())
            .await;
        let err = result.expect_err("replaced message should be rejected");
        assert!(err.to_string().contains("different CID"), "{err}");
    }

    /// The same replacing message is accepted when replacements are allowed,
    /// returning the receipt at the head tipset.
    #[tokio::test]
    async fn wait_for_message_accepts_replaced_when_allowed() {
        let db = Arc::new(MemoryDB::default());
        let (state_manager, msg_cid) = state_manager_with_replaced_message_at_head(&db);

        let (tipset, receipt) = state_manager
            .wait_for_message(msg_cid, 0, None, Some(true), &CancellationToken::new())
            .await
            .expect("replaced message should be accepted");
        assert_eq!(tipset.epoch(), 3);
        assert!(receipt.exit_code().is_success());
    }
}

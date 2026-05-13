// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Reorg handling: revert + apply tipsets against the pending pool.

use ahash::{HashMap, HashMapExt};
use tracing::error;

use crate::blocks::Tipset;
use crate::message::{MessageRead as _, SignedMessage};
use crate::message_pool::msgpool::utils;
use crate::message_pool::{
    Error,
    msg_pool::{StrictnessPolicy, TrustPolicy},
    msgpool::{msg_pool::MessagePool, recover_sig},
    provider::Provider,
};
use crate::shim::address::Address;
use crate::utils::ShallowClone as _;

impl<T> MessagePool<T>
where
    T: Provider + 'static,
{
    /// Revert and/or apply tipsets to the message pool.
    ///
    /// - **Apply**: messages included in the new tipset are removed from the
    ///   pending pool with `applied = true`.
    /// - **Revert**: messages from the reverted tipset are re-added to the
    ///   pool with [`StrictnessPolicy::Relaxed`] and [`TrustPolicy::Trusted`],
    ///   allowing them back without nonce-gap restrictions.
    ///
    /// The state-nonce cache is naturally invalidated when the tipset
    /// changes, since it is keyed by `(TipsetKey, Address)`.
    pub(in crate::message_pool) async fn apply_head_change(
        &self,
        revert: Vec<Tipset>,
        apply: Vec<Tipset>,
    ) -> Result<(), Error> {
        let mut repub = false;
        let mut rmsgs: HashMap<Address, HashMap<u64, SignedMessage>> = HashMap::new();
        for ts in revert {
            let Ok(pts) = self.api.load_tipset(ts.parents()) else {
                tracing::error!("error loading reverted tipset parent");
                continue;
            };
            *self.cur_tipset.write() = pts;

            let mut msgs: Vec<SignedMessage> = Vec::new();
            for block in ts.block_headers() {
                let Ok((umsg, smsgs)) = self.api.messages_for_block(block) else {
                    tracing::error!("error retrieving messages for reverted block");
                    continue;
                };
                msgs.extend(smsgs);
                for msg in umsg {
                    let msg_cid = msg.cid();
                    let Ok(smsg) = recover_sig(&self.caches.bls_sig, msg) else {
                        tracing::debug!("could not recover signature for bls message {}", msg_cid);
                        continue;
                    };
                    msgs.push(smsg)
                }
            }

            for msg in msgs {
                utils::add_to_selected_msgs(msg, &mut rmsgs);
            }
        }

        for ts in apply {
            for b in ts.block_headers() {
                let Ok((msgs, smsgs)) = self.api.messages_for_block(b) else {
                    tracing::error!("error retrieving messages for block");
                    continue;
                };

                for msg in smsgs {
                    self.remove_applied_from_pool(&msg.from(), msg.sequence(), &mut rmsgs, &ts)?;
                    if !repub && self.republish.was_republished(&msg.cid()) {
                        repub = true;
                    }
                }
                for msg in msgs {
                    self.remove_applied_from_pool(&msg.from, msg.sequence, &mut rmsgs, &ts)?;
                    if !repub && self.republish.was_republished(&msg.cid()) {
                        repub = true;
                    }
                }
            }
            *self.cur_tipset.write() = ts;
        }
        if repub {
            self.republish.trigger()?;
        }

        let cur_ts = self.cur_tipset.read().shallow_clone();
        for (_, hm) in rmsgs {
            for (_, msg) in hm {
                if let Err(e) = self.add_to_pool_unchecked(
                    &cur_ts,
                    msg,
                    TrustPolicy::Trusted,
                    StrictnessPolicy::Relaxed,
                ) {
                    error!("Failed to read message from reorg to mpool: {}", e);
                }
            }
        }
        Ok(())
    }

    /// Remove a message from the in-progress `rmsgs` scratch map. If the
    /// message isn't there, fall back to removing it from the real pending
    /// pool. Used by [`Self::apply_head_change`] when an applied tipset
    /// includes a message that we hadn't yet seen reverted.
    fn remove_applied_from_pool(
        &self,
        from: &Address,
        sequence: u64,
        rmsgs: &mut HashMap<Address, HashMap<u64, SignedMessage>>,
        ts: &Tipset,
    ) -> Result<(), Error> {
        if rmsgs
            .get_mut(from)
            .and_then(|temp| temp.remove(&sequence))
            .is_none()
            && let Ok(resolved) = self
                .resolve_to_key(from, ts)
                .inspect_err(|e| tracing::debug!(%from, "remove: failed to resolve address: {e:#}"))
        {
            let _ = self.pending.remove(&resolved, sequence, true);
        }
        Ok(())
    }
}

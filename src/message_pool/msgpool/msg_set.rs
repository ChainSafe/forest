// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Per-sender message set.
//!
//! [`MsgSet`] owns the pending messages for a single sender address and tracks
//! the next sequence expected for the gap-filling / replace-by-fee rules. It is
//! deliberately decoupled from the [`super::Provider`] trait: callers pass
//! explicit [`MsgSetLimits`] so this type (and its tests) need no mock
//! provider.

use ahash::{HashMap, HashMapExt};

use crate::message::{MessageRead, SignedMessage};
use crate::message_pool::errors::Error;
use crate::message_pool::metrics;
use crate::message_pool::msgpool::{RBF_DENOM, RBF_NUM, TrustPolicy};
use crate::shim::econ::TokenAmount;

/// Maximum allowed nonce gap for trusted message inserts under [`StrictnessPolicy::Strict`].
pub(in crate::message_pool) const MAX_NONCE_GAP: u64 = 4;

/// Per-actor pending-message limits for [`MsgSet::add`].
#[derive(Clone, Copy, Debug)]
pub struct MsgSetLimits {
    /// Cap applied when a message is inserted via the trusted path.
    pub trusted: u64,
    /// Cap applied when a message is inserted via the untrusted path.
    pub untrusted: u64,
}

impl MsgSetLimits {
    pub fn new(trusted: u64, untrusted: u64) -> Self {
        Self { trusted, untrusted }
    }
}

/// Strictness policy for pending insertion; enforces nonce-gap and
/// replace-by-fee-during-gap rules when [`StrictnessPolicy::Strict`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrictnessPolicy {
    Strict,
    Relaxed,
}

/// Simple structure that contains a hash-map of messages where k: a message
/// from address, v: a message which corresponds to that address.
#[derive(Clone, Default, Debug)]
pub struct MsgSet {
    pub(in crate::message_pool) msgs: HashMap<u64, SignedMessage>,
    pub(in crate::message_pool) next_sequence: u64,
}

impl MsgSet {
    /// Generate a new `MsgSet` with an empty hash-map and setting the sequence
    /// specifically.
    pub fn new(sequence: u64) -> Self {
        MsgSet {
            msgs: HashMap::new(),
            next_sequence: sequence,
        }
    }

    /// Insert a message into this set, maintaining `next_sequence`.
    ///
    /// - If the message nonce equals `next_sequence`, advance past any
    ///   consecutive existing messages (gap-filling loop).
    /// - If the nonce exceeds `next_sequence + max_nonce_gap` and [`StrictnessPolicy::Strict`],
    ///   reject with [`Error::NonceGap`].
    /// - Replace-by-fee for an existing nonce is rejected when strict and
    ///   a nonce gap is present.
    ///
    /// [`StrictnessPolicy`] and [`TrustPolicy`] are independent: strictness controls
    /// whether nonce gap checks run, while [`TrustPolicy`] sets `max_nonce_gap`
    /// ([`MAX_NONCE_GAP`] for trusted, `0` for untrusted) and selects which cap
    /// in [`MsgSetLimits`] applies.
    pub(in crate::message_pool) fn add(
        &mut self,
        m: SignedMessage,
        strictness: StrictnessPolicy,
        trust: TrustPolicy,
        limits: MsgSetLimits,
    ) -> Result<(), Error> {
        let strict = matches!(strictness, StrictnessPolicy::Strict);
        let trusted = matches!(trust, TrustPolicy::Trusted);
        let max_nonce_gap: u64 = if trusted { MAX_NONCE_GAP } else { 0 };
        let max_actor_pending_messages = if trusted {
            limits.trusted
        } else {
            limits.untrusted
        };

        let mut next_nonce = self.next_sequence;
        let nonce_gap = if m.sequence() == next_nonce {
            next_nonce += 1;
            while self.msgs.contains_key(&next_nonce) {
                next_nonce += 1;
            }
            false
        } else if strict && m.sequence() > next_nonce + max_nonce_gap {
            tracing::debug!(
                nonce = m.sequence(),
                next_nonce,
                "message nonce has too big a gap from expected nonce"
            );
            return Err(Error::NonceGap);
        } else {
            m.sequence() > next_nonce
        };

        let has_existing = if let Some(exms) = self.msgs.get(&m.sequence()) {
            if strict && nonce_gap {
                tracing::debug!(
                    nonce = m.sequence(),
                    next_nonce,
                    "rejecting replace by fee because of nonce gap"
                );
                return Err(Error::NonceGap);
            }
            if m.cid() != exms.cid() {
                let premium = &exms.message().gas_premium;
                let min_price = premium.clone()
                    + ((premium * RBF_NUM).div_floor(RBF_DENOM))
                    + TokenAmount::from_atto(1u8);
                if m.message().gas_premium <= min_price {
                    return Err(Error::GasPriceTooLow);
                }
            } else {
                return Err(Error::DuplicateSequence);
            }
            true
        } else {
            false
        };

        // Only check the limit when adding a new message, not when replacing an existing one (RBF)
        if !has_existing && self.msgs.len() as u64 >= max_actor_pending_messages {
            return Err(Error::TooManyPendingMessages(
                m.message.from().to_string(),
                trusted,
            ));
        }

        if strict && nonce_gap {
            tracing::debug!(
                from = %m.from(),
                nonce = m.sequence(),
                next_nonce,
                "adding nonce-gapped message"
            );
        }

        self.next_sequence = next_nonce;
        if self.msgs.insert(m.sequence(), m).is_none() {
            metrics::MPOOL_MESSAGE_TOTAL.inc();
        }
        Ok(())
    }

    /// Remove the message at `sequence` and adjust `next_sequence`.
    ///
    /// - **Applied** (included on-chain): advance `next_sequence` to
    ///   `sequence + 1` if needed. For messages not in our pool, also run
    ///   the gap-filling loop to advance past consecutive known messages.
    /// - **Pruned** (evicted): rewind `next_sequence` to `sequence` if the
    ///   removal creates a gap.
    ///
    /// Returns the removed message when one was present.
    /// If the sequence was not in the set, no event is removed and [`None`] is returned.
    pub fn rm(&mut self, sequence: u64, applied: bool) -> Option<SignedMessage> {
        let Some(removed) = self.msgs.remove(&sequence) else {
            if applied && sequence >= self.next_sequence {
                self.next_sequence = sequence + 1;
                while self.msgs.contains_key(&self.next_sequence) {
                    self.next_sequence += 1;
                }
            }
            return None;
        };
        metrics::MPOOL_MESSAGE_TOTAL.dec();

        // adjust next sequence
        if applied {
            // we removed a (known) message because it was applied in a tipset
            // we can't possibly have filled a gap in this case
            if sequence >= self.next_sequence {
                self.next_sequence = sequence + 1;
            }
        } else if sequence < self.next_sequence {
            // we removed a message because it was pruned
            // we have to adjust the sequence if it creates a gap or rewinds state
            self.next_sequence = sequence;
        }
        Some(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shim::address::Address;
    use crate::shim::econ::TokenAmount;
    use crate::shim::message::Message as ShimMessage;

    fn make_smsg(from: Address, seq: u64, premium: u64) -> SignedMessage {
        SignedMessage::mock_bls_signed_message(ShimMessage {
            from,
            sequence: seq,
            gas_premium: TokenAmount::from_atto(premium),
            gas_limit: 1_000_000,
            ..ShimMessage::default()
        })
    }

    // Test that RBF (Replace By Fee) is allowed even when at max_actor_pending_messages capacity
    // This matches Lotus behavior where the check is: https://github.com/filecoin-project/lotus/blob/5f32d00550ddd2f2d0f9abe97dbae07615f18547/chain/messagepool/messagepool.go#L296-L299
    #[test]
    fn rbf_at_capacity() {
        let limits = MsgSetLimits::new(10, 10);
        let mut mset = MsgSet::new(0);

        // Fill up to capacity (10 messages)
        for i in 0..10 {
            let res = mset.add(
                make_smsg(Address::default(), i, 100),
                StrictnessPolicy::Relaxed,
                TrustPolicy::Trusted,
                limits,
            );
            assert!(res.is_ok(), "Failed to add message {i}");
        }

        // Should reject adding a NEW message (sequence 10) when at capacity
        let res = mset.add(
            make_smsg(Address::default(), 10, 100),
            StrictnessPolicy::Relaxed,
            TrustPolicy::Trusted,
            limits,
        );
        assert!(matches!(res, Err(Error::TooManyPendingMessages(_, _))));

        // Should ALLOW replacing an existing message (RBF) even when at capacity
        // Replace message with sequence 5 with higher gas premium
        let res = mset.add(
            make_smsg(Address::default(), 5, 200),
            StrictnessPolicy::Relaxed,
            TrustPolicy::Trusted,
            limits,
        );
        assert!(res.is_ok(), "RBF should be allowed at capacity");
    }

    #[test]
    fn gap_filling_advances_next_sequence() {
        let limits = MsgSetLimits::new(1000, 1000);
        let mut mset = MsgSet::new(0);

        mset.add(
            make_smsg(Address::default(), 0, 100),
            StrictnessPolicy::Relaxed,
            TrustPolicy::Trusted,
            limits,
        )
        .unwrap();
        assert_eq!(mset.next_sequence, 1);

        mset.add(
            make_smsg(Address::default(), 2, 100),
            StrictnessPolicy::Relaxed,
            TrustPolicy::Trusted,
            limits,
        )
        .unwrap();
        assert_eq!(mset.next_sequence, 1, "gap at 1, so next_sequence stays");

        mset.add(
            make_smsg(Address::default(), 1, 100),
            StrictnessPolicy::Relaxed,
            TrustPolicy::Trusted,
            limits,
        )
        .unwrap();
        assert_eq!(
            mset.next_sequence, 3,
            "filling the gap should advance past all consecutive messages"
        );
    }

    #[test]
    fn trusted_allows_any_nonce_gap() {
        let limits = MsgSetLimits::new(1000, 1000);
        let mut mset = MsgSet::new(0);

        mset.add(
            make_smsg(Address::default(), 0, 100),
            StrictnessPolicy::Relaxed,
            TrustPolicy::Trusted,
            limits,
        )
        .unwrap();
        let res = mset.add(
            make_smsg(Address::default(), 10, 100),
            StrictnessPolicy::Relaxed,
            TrustPolicy::Trusted,
            limits,
        );
        assert!(
            res.is_ok(),
            "trusted adds skip nonce gap enforcement (StrictnessPolicy::Relaxed)"
        );
    }

    #[test]
    fn strict_allows_small_nonce_gap() {
        let limits = MsgSetLimits::new(1000, 1000);
        let mut mset = MsgSet::new(0);

        // Strict + trusted -> max_nonce_gap=4 (non-local add path)
        mset.add(
            make_smsg(Address::default(), 0, 100),
            StrictnessPolicy::Strict,
            TrustPolicy::Trusted,
            limits,
        )
        .unwrap();
        let res = mset.add(
            make_smsg(Address::default(), 3, 100),
            StrictnessPolicy::Strict,
            TrustPolicy::Trusted,
            limits,
        );
        assert!(
            res.is_ok(),
            "strict+trusted: gap of 2 (within MAX_NONCE_GAP=4) should succeed"
        );
    }

    #[test]
    fn strict_rejects_large_nonce_gap() {
        let limits = MsgSetLimits::new(1000, 1000);
        let mut mset = MsgSet::new(0);

        // Strict + trusted -> max_nonce_gap=4
        mset.add(
            make_smsg(Address::default(), 0, 100),
            StrictnessPolicy::Strict,
            TrustPolicy::Trusted,
            limits,
        )
        .unwrap();
        let res = mset.add(
            make_smsg(Address::default(), 6, 100),
            StrictnessPolicy::Strict,
            TrustPolicy::Trusted,
            limits,
        );
        assert_eq!(
            res,
            Err(Error::NonceGap),
            "strict+trusted: gap of 5 (exceeds MAX_NONCE_GAP=4) should be rejected"
        );
    }

    #[test]
    fn strict_untrusted_rejects_any_gap() {
        let limits = MsgSetLimits::new(1000, 1000);
        let mut mset = MsgSet::new(0);

        // Strict + untrusted -> max_nonce_gap=0
        mset.add(
            make_smsg(Address::default(), 0, 100),
            StrictnessPolicy::Strict,
            TrustPolicy::Untrusted,
            limits,
        )
        .unwrap();
        let res = mset.add(
            make_smsg(Address::default(), 2, 100),
            StrictnessPolicy::Strict,
            TrustPolicy::Untrusted,
            limits,
        );
        assert_eq!(
            res,
            Err(Error::NonceGap),
            "strict+untrusted: any gap (maxNonceGap=0) is rejected"
        );
    }

    #[test]
    fn non_strict_untrusted_skips_gap_check() {
        let limits = MsgSetLimits::new(1000, 1000);
        let mut mset = MsgSet::new(0);

        // Relaxed + untrusted -> gap check skipped (PushUntrusted path)
        mset.add(
            make_smsg(Address::default(), 0, 100),
            StrictnessPolicy::Relaxed,
            TrustPolicy::Untrusted,
            limits,
        )
        .unwrap();
        let res = mset.add(
            make_smsg(Address::default(), 5, 100),
            StrictnessPolicy::Relaxed,
            TrustPolicy::Untrusted,
            limits,
        );
        assert!(
            res.is_ok(),
            "non-strict untrusted (PushUntrusted) skips gap enforcement"
        );
    }

    #[test]
    fn strict_rbf_during_gap_rejected() {
        let limits = MsgSetLimits::new(1000, 1000);
        let mut mset = MsgSet::new(0);

        // Set up a gap using relaxed trusted (local push path)
        mset.add(
            make_smsg(Address::default(), 0, 100),
            StrictnessPolicy::Relaxed,
            TrustPolicy::Trusted,
            limits,
        )
        .unwrap();
        mset.add(
            make_smsg(Address::default(), 2, 100),
            StrictnessPolicy::Relaxed,
            TrustPolicy::Trusted,
            limits,
        )
        .unwrap();

        // Strict RBF at nonce 2 should be rejected due to gap at nonce 1
        let res = mset.add(
            make_smsg(Address::default(), 2, 200),
            StrictnessPolicy::Strict,
            TrustPolicy::Trusted,
            limits,
        );
        assert_eq!(
            res,
            Err(Error::NonceGap),
            "strict RBF should be rejected when nonce gap exists"
        );
    }

    #[test]
    fn rbf_without_gap_still_works() {
        let limits = MsgSetLimits::new(1000, 1000);
        let mut mset = MsgSet::new(0);

        mset.add(
            make_smsg(Address::default(), 0, 100),
            StrictnessPolicy::Relaxed,
            TrustPolicy::Trusted,
            limits,
        )
        .unwrap();
        mset.add(
            make_smsg(Address::default(), 1, 100),
            StrictnessPolicy::Relaxed,
            TrustPolicy::Trusted,
            limits,
        )
        .unwrap();
        mset.add(
            make_smsg(Address::default(), 2, 100),
            StrictnessPolicy::Relaxed,
            TrustPolicy::Trusted,
            limits,
        )
        .unwrap();

        let res = mset.add(
            make_smsg(Address::default(), 1, 200),
            StrictnessPolicy::Relaxed,
            TrustPolicy::Trusted,
            limits,
        );
        assert!(res.is_ok(), "RBF without a nonce gap should succeed");
    }

    #[test]
    fn rm_applied_advances_next_sequence() {
        let limits = MsgSetLimits::new(1000, 1000);
        let mut mset = MsgSet::new(0);

        mset.add(
            make_smsg(Address::default(), 0, 100),
            StrictnessPolicy::Relaxed,
            TrustPolicy::Trusted,
            limits,
        )
        .unwrap();
        assert_eq!(mset.next_sequence, 1);

        // applied=true, and sequence >= next_sequence path: remove advances
        mset.rm(0, true);
        assert_eq!(
            mset.next_sequence, 1,
            "applied rm at seq < next_sequence does not advance further"
        );

        // applied=true with an unknown sequence ahead of current: advances
        mset.rm(5, true);
        assert_eq!(
            mset.next_sequence, 6,
            "applied rm of unknown seq >= next_sequence advances to seq+1"
        );
    }

    #[test]
    fn rm_pruned_rewinds_next_sequence_on_gap() {
        let limits = MsgSetLimits::new(1000, 1000);
        let mut mset = MsgSet::new(0);

        // Fill 0..=2 so next_sequence=3
        for i in 0..3 {
            mset.add(
                make_smsg(Address::default(), i, 100),
                StrictnessPolicy::Relaxed,
                TrustPolicy::Trusted,
                limits,
            )
            .unwrap();
        }
        assert_eq!(mset.next_sequence, 3);

        // applied=false (prune) of seq=1 (< next_sequence): rewind to 1
        mset.rm(1, false);
        assert_eq!(
            mset.next_sequence, 1,
            "pruned rm creating a gap rewinds next_sequence"
        );
    }
}

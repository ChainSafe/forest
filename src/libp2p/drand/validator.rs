// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::{Arc, LazyLock};

use libp2p::{
    PeerId,
    gossipsub::{MessageAcceptance, MessageId},
};
use nonzero_ext::nonzero;
use quick_protobuf::{BytesReader, MessageRead as _};
use tokio::sync::Semaphore;
use tracing::debug;

use super::{
    Verdict,
    metrics::{self, Reason},
};
use crate::beacon::{Beacon as _, BeaconEntry, BeaconSchedule, PublicRandResponse};
use crate::utils::misc::env::env_or_default_logged;

/// Older gossiped rounds are ignored; accepting them would let peers fill the cache.
pub(in crate::libp2p) const MAX_ROUND_AGE_SECS: u64 = 30;

/// Rounds further ahead are ignored, not rejected: a slow local clock must not penalize peers.
pub(in crate::libp2p) const MAX_CLOCK_SKEW_SECS: u64 = 3;

/// Concurrency cap for `drand` entries verified from `gossipsub`. Excess is dropped, not queued.
static MAX_CONCURRENT_VERIFICATIONS: LazyLock<usize> = LazyLock::new(|| {
    env_or_default_logged(
        "FOREST_MAX_CONCURRENT_DRAND_VERIFICATIONS",
        nonzero!(4_usize),
    )
    .get()
    .min(Semaphore::MAX_PERMITS)
});

/// Verifies gossiped `drand` entries, reporting a verdict for each. Only malformed payloads and bad
/// signatures are rejected; anything else unverified is ignored.
pub(in crate::libp2p) struct DrandGossipValidator {
    schedule: Arc<BeaconSchedule>,
    limiter: Arc<Semaphore>,
    verdicts: flume::Sender<Verdict>,
}

impl DrandGossipValidator {
    pub(in crate::libp2p) fn new(
        schedule: Arc<BeaconSchedule>,
        verdicts: flume::Sender<Verdict>,
    ) -> Self {
        Self::with_max_verifications(schedule, verdicts, *MAX_CONCURRENT_VERIFICATIONS)
    }

    pub(in crate::libp2p) fn with_max_verifications(
        schedule: Arc<BeaconSchedule>,
        verdicts: flume::Sender<Verdict>,
        max_verifications: usize,
    ) -> Self {
        Self {
            schedule,
            limiter: Arc::new(Semaphore::new(max_verifications)),
            verdicts,
        }
    }

    /// Validates a `PublicRandResponse` payload received at Unix time `now`, sending exactly one
    /// [`Verdict`].
    pub(in crate::libp2p) fn handle(
        &self,
        data: &[u8],
        message_id: MessageId,
        source: PeerId,
        now: u64,
    ) {
        let verdict = |acceptance, reason| {
            report(
                &self.verdicts,
                message_id.clone(),
                source,
                acceptance,
                reason,
            )
        };
        let mut reader = BytesReader::from_bytes(data);
        let Ok(response) = PublicRandResponse::from_reader(&mut reader, data) else {
            debug!(%source, "drand entry could not be decoded");
            return verdict(MessageAcceptance::Reject, Reason::Malformed);
        };
        let entry = BeaconEntry::new(response.round, response.signature);
        let round = entry.round();

        let Some(beacon) = self.schedule.unchained_beacon() else {
            return verdict(MessageAcceptance::Ignore, Reason::NoBeacon);
        };
        if round == 0 || entry.signature().is_empty() {
            return verdict(MessageAcceptance::Reject, Reason::Malformed);
        }
        if let Some(round_ts) = beacon.beacon_round_timestamp(round) {
            if round_ts > now.saturating_add(MAX_CLOCK_SKEW_SECS) {
                debug!(round, %source, "ignoring drand entry: round is in the future");
                return verdict(MessageAcceptance::Ignore, Reason::Future);
            }
            if round_ts.saturating_add(MAX_ROUND_AGE_SECS) < now {
                debug!(round, %source, "ignoring drand entry: round is stale");
                return verdict(MessageAcceptance::Ignore, Reason::Stale);
            }
        }

        let Ok(permit) = Arc::clone(&self.limiter).try_acquire_owned() else {
            debug!(
                round,
                "ignoring drand entry: too many verifications in flight"
            );
            return verdict(MessageAcceptance::Ignore, Reason::NoSlot);
        };
        let schedule = Arc::clone(&self.schedule);
        let verdicts = self.verdicts.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let verified = schedule.unchained_beacon().is_some_and(|beacon| {
                matches!(
                    beacon.verify_entries(std::slice::from_ref(&entry), &BeaconEntry::default()),
                    Ok(true)
                )
            });
            let (acceptance, reason) = if verified {
                debug!(round, "verified drand entry from gossipsub");
                (MessageAcceptance::Accept, Reason::Verified)
            } else {
                debug!(round, %source, "received invalid drand entry over gossipsub");
                (MessageAcceptance::Reject, Reason::InvalidSignature)
            };
            report(&verdicts, message_id, source, acceptance, reason);
        });
    }
}

fn report(
    verdicts: &flume::Sender<Verdict>,
    message_id: MessageId,
    source: PeerId,
    acceptance: MessageAcceptance,
    reason: Reason,
) {
    metrics::record(&acceptance, reason);
    // Fails only once the service is gone.
    let _ = verdicts.send(Verdict {
        message_id,
        source,
        acceptance,
    });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::beacon::tests::fake_drand_signer::FakeDrandSigner;
    use crate::libp2p::drand::test_utils::{
        EPOCH_ROUND, assert_verdict, fake_drand_schedule, is_cached, time_of,
    };

    /// Runs `payload` through a validator with `max_verifications` slots.
    async fn verdict_for(
        schedule: &Arc<BeaconSchedule>,
        payload: &[u8],
        now: u64,
        max_verifications: usize,
    ) -> MessageAcceptance {
        let (verdicts_tx, verdicts) = flume::unbounded();
        let validator = DrandGossipValidator::with_max_verifications(
            schedule.clone(),
            verdicts_tx,
            max_verifications,
        );
        validator.handle(payload, MessageId::new(b"drand"), PeerId::random(), now);
        tokio::time::timeout(Duration::from_secs(5), verdicts.recv_async())
            .await
            .expect("no verdict reported in time")
            .expect("verdict channel closed")
            .acceptance
    }

    /// Valid epoch rounds are accepted and cached.
    #[rstest::rstest]
    #[case::epoch_round(EPOCH_ROUND, 0)]
    // Within the clock skew allowance.
    #[case::produced_slightly_ahead(EPOCH_ROUND, -(MAX_CLOCK_SKEW_SECS as i64))]
    #[tokio::test]
    async fn valid_round_is_accepted(#[case] round: u64, #[case] now_offset: i64) {
        let (drand, schedule) = fake_drand_schedule();
        let now = time_of(&schedule, round).saturating_add_signed(now_offset);

        let verdict = verdict_for(&schedule, &drand.to_protobuf(round), now, 1).await;

        assert_verdict(&verdict, MessageAcceptance::Accept);
        assert!(is_cached(&schedule, round), "round {round}");
    }

    /// Entries rejected or ignored without being cached.
    #[rstest::rstest]
    #[case::undecodable(|_: &FakeDrandSigner| vec![0xff, 0xff, 0xff], 0, MessageAcceptance::Reject)]
    #[case::round_zero(|d: &FakeDrandSigner| d.to_protobuf_raw(0, d.entry(1).signature()), 0, MessageAcceptance::Reject)]
    #[case::empty_signature(|d: &FakeDrandSigner| d.to_protobuf_raw(EPOCH_ROUND, &[]), 0, MessageAcceptance::Reject)]
    // An epoch round with the next round's signature.
    #[case::forged_signature(
        |d: &FakeDrandSigner| d.to_protobuf_raw(EPOCH_ROUND, d.entry(EPOCH_ROUND + 1).signature()),
        0,
        MessageAcceptance::Reject
    )]
    // Beyond the clock skew allowance.
    #[case::future_round(|d: &FakeDrandSigner| d.to_protobuf(EPOCH_ROUND + 2), 0, MessageAcceptance::Ignore)]
    #[case::stale_round(|d: &FakeDrandSigner| d.to_protobuf(EPOCH_ROUND), MAX_ROUND_AGE_SECS as i64, MessageAcceptance::Ignore)]
    #[tokio::test]
    async fn invalid_or_untimely_entry_is_not_accepted(
        #[case] payload: fn(&FakeDrandSigner) -> Vec<u8>,
        #[case] now_offset: i64,
        #[case] expected: MessageAcceptance,
    ) {
        let (drand, schedule) = fake_drand_schedule();
        let now = time_of(&schedule, EPOCH_ROUND).saturating_add_signed(now_offset);

        let verdict = verdict_for(&schedule, &payload(&drand), now, 1).await;

        assert_verdict(&verdict, expected);
        assert!(!is_cached(&schedule, EPOCH_ROUND), "nothing cached");
    }

    #[tokio::test]
    async fn entry_ignored_when_verifications_saturated() {
        let (drand, schedule) = fake_drand_schedule();
        let now = time_of(&schedule, EPOCH_ROUND);

        let verdict = verdict_for(&schedule, &drand.to_protobuf(EPOCH_ROUND), now, 0).await;

        assert_verdict(&verdict, MessageAcceptance::Ignore);
    }
}

// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use libp2p::{
    PeerId,
    gossipsub::{MessageAcceptance, MessageId},
};
use quick_protobuf::{BytesReader, MessageRead as _};
use tracing::debug;

use super::{
    Verdict,
    metrics::{self, Reason},
};
use crate::beacon::{Beacon as _, BeaconEntry, BeaconSchedule, PublicRandResponse};

/// Older gossiped rounds are ignored; accepting them would let peers fill the cache.
const MAX_ROUND_AGE_SECS: u64 = 30;

/// Verifies gossiped `drand` entries, reporting a verdict for each. Only malformed payloads and bad
/// signatures are rejected; anything else unverified is ignored.
pub(in crate::libp2p) struct DrandGossipValidator {
    schedule: Arc<BeaconSchedule>,
    verdicts: flume::Sender<Verdict>,
}

impl DrandGossipValidator {
    pub(in crate::libp2p) fn new(
        schedule: Arc<BeaconSchedule>,
        verdicts: flume::Sender<Verdict>,
    ) -> Self {
        Self { schedule, verdicts }
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

        if round == 0 || entry.signature().is_empty() {
            return verdict(MessageAcceptance::Reject, Reason::Malformed);
        }

        let beacon = self
            .schedule
            .unchained_beacon()
            .expect("drand gossip requires an unchained beacon");

        if beacon
            .round_timestamp(round)
            .saturating_add(MAX_ROUND_AGE_SECS)
            < now
        {
            debug!(round, %source, "ignoring drand entry: round is stale");
            return verdict(MessageAcceptance::Ignore, Reason::Stale);
        }

        // Like drand's own validator: a round we hold is not forwarded again, and a different
        // signature for it is a forgery.
        if beacon.is_verified(&entry) {
            return verdict(MessageAcceptance::Ignore, Reason::Duplicate);
        }
        if beacon.has_round(round) {
            return verdict(MessageAcceptance::Reject, Reason::InvalidSignature);
        }

        let schedule = Arc::clone(&self.schedule);
        let verdicts = self.verdicts.clone();
        tokio::task::spawn_blocking(move || {
            let (acceptance, reason) = match schedule
                .unchained_beacon()
                .expect("drand gossip requires an unchained beacon")
                .verify_entries(std::slice::from_ref(&entry), &BeaconEntry::default())
            {
                Ok(true) => (MessageAcceptance::Accept, Reason::Verified),
                Ok(false) => (MessageAcceptance::Reject, Reason::InvalidSignature),
                Err(_) => (MessageAcceptance::Reject, Reason::Malformed),
            };
            debug!(round, %source, ?reason, "checked drand entry from gossipsub");
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
    // Never block: the service loop that drains the channel also calls this.
    if let Err(e) = verdicts.try_send(Verdict {
        message_id,
        source,
        acceptance,
    }) {
        debug!(%source, "dropping drand verdict: {e}");
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::beacon::tests::fake_drand_signer::FakeDrandSigner;
    use crate::libp2p::drand::test_utils::{
        EPOCH_ROUND, assert_verdict, fake_drand_schedule, is_cached, time_of,
    };

    fn validator(
        schedule: &Arc<BeaconSchedule>,
    ) -> (DrandGossipValidator, flume::Receiver<Verdict>) {
        let (verdicts_tx, verdicts) = flume::unbounded();
        (
            DrandGossipValidator::new(schedule.clone(), verdicts_tx),
            verdicts,
        )
    }

    async fn verdict_for(
        schedule: &Arc<BeaconSchedule>,
        payload: &[u8],
        now: u64,
    ) -> MessageAcceptance {
        let (validator, verdicts) = validator(schedule);
        validator.handle(payload, MessageId::new(b"drand"), PeerId::random(), now);
        tokio::time::timeout(Duration::from_secs(5), verdicts.recv_async())
            .await
            .expect("no verdict reported in time")
            .expect("verdict channel closed")
            .acceptance
    }

    /// The verdict decided inline, before any verification task is spawned.
    fn immediate_verdict_for(
        schedule: &Arc<BeaconSchedule>,
        payload: &[u8],
        now: u64,
    ) -> MessageAcceptance {
        let (validator, verdicts) = validator(schedule);
        validator.handle(payload, MessageId::new(b"drand"), PeerId::random(), now);
        verdicts
            .try_recv()
            .expect("an immediate verdict")
            .acceptance
    }

    /// Valid epoch rounds are accepted and cached.
    #[rstest::rstest]
    #[case::epoch_round(EPOCH_ROUND, 0)]
    // A slow local clock must not lose rounds.
    #[case::ahead_of_local_clock(EPOCH_ROUND, -10)]
    #[tokio::test]
    async fn valid_round_is_accepted(#[case] round: u64, #[case] now_offset: i64) {
        let (drand, schedule) = fake_drand_schedule();
        let now = time_of(&schedule, round).saturating_add_signed(now_offset);

        let verdict = verdict_for(&schedule, &drand.encode(round), now).await;

        assert_verdict(&verdict, MessageAcceptance::Accept);
        assert!(is_cached(&schedule, round), "round {round}");
    }

    /// Entries rejected or ignored without being cached.
    #[rstest::rstest]
    #[case::undecodable(|_: &FakeDrandSigner| vec![0xff, 0xff, 0xff], 0, MessageAcceptance::Reject)]
    #[case::round_zero(|d: &FakeDrandSigner| FakeDrandSigner::encode_raw(0, d.entry(1).signature()), 0, MessageAcceptance::Reject)]
    #[case::empty_signature(|_: &FakeDrandSigner| FakeDrandSigner::encode_raw(EPOCH_ROUND, &[]), 0, MessageAcceptance::Reject)]
    #[case::wrong_length_signature(|_: &FakeDrandSigner| FakeDrandSigner::encode_raw(EPOCH_ROUND, &[1, 2, 3]), 0, MessageAcceptance::Reject)]
    // An epoch round with the next round's signature.
    #[case::forged_signature(
        |d: &FakeDrandSigner| FakeDrandSigner::encode_raw(EPOCH_ROUND, d.entry(EPOCH_ROUND + 1).signature()),
        0,
        MessageAcceptance::Reject
    )]
    #[case::stale_round(|d: &FakeDrandSigner| d.encode(EPOCH_ROUND), MAX_ROUND_AGE_SECS as i64, MessageAcceptance::Ignore)]
    #[tokio::test]
    async fn invalid_or_untimely_entry_is_not_accepted(
        #[case] payload: fn(&FakeDrandSigner) -> Vec<u8>,
        #[case] now_offset: i64,
        #[case] expected: MessageAcceptance,
    ) {
        let (drand, schedule) = fake_drand_schedule();
        let now = time_of(&schedule, EPOCH_ROUND).saturating_add_signed(now_offset);

        let verdict = verdict_for(&schedule, &payload(&drand), now).await;

        assert_verdict(&verdict, expected);
        assert!(!is_cached(&schedule, EPOCH_ROUND), "nothing cached");
    }

    /// Another relay, or an HTTP fetch, may already have cached the round.
    #[tokio::test]
    async fn cached_round_is_not_forwarded_again() {
        let (drand, schedule) = fake_drand_schedule();
        let now = time_of(&schedule, EPOCH_ROUND);
        let payload = drand.encode(EPOCH_ROUND);

        let verdict = verdict_for(&schedule, &payload, now).await;
        assert_verdict(&verdict, MessageAcceptance::Accept);
        let verdict = immediate_verdict_for(&schedule, &payload, now);
        assert_verdict(&verdict, MessageAcceptance::Ignore);
    }

    #[tokio::test]
    async fn cached_round_with_another_signature_is_rejected() {
        let (drand, schedule) = fake_drand_schedule();
        let now = time_of(&schedule, EPOCH_ROUND);
        let forged =
            FakeDrandSigner::encode_raw(EPOCH_ROUND, drand.entry(EPOCH_ROUND + 1).signature());

        let verdict = verdict_for(&schedule, &drand.encode(EPOCH_ROUND), now).await;
        assert_verdict(&verdict, MessageAcceptance::Accept);
        let verdict = immediate_verdict_for(&schedule, &forged, now);
        assert_verdict(&verdict, MessageAcceptance::Reject);

        let beacon = schedule.unchained_beacon().expect("unchained beacon");
        assert_eq!(
            beacon.entry(EPOCH_ROUND).await.unwrap(),
            drand.entry(EPOCH_ROUND),
            "the cache keeps the valid entry"
        );
    }
}

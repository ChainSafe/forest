// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod metrics;

use std::sync::Arc;

use libp2p::{
    PeerId,
    gossipsub::{MessageAcceptance, MessageId, TopicHash},
};
use quick_protobuf::{BytesReader, MessageRead as _};
use tracing::debug;

use super::{Beacon as _, BeaconEntry, BeaconSchedule, PublicRandResponse};
use crate::libp2p::Verdict;
use metrics::Reason;

/// `gossipsub` topic prefix of a `drand` network, followed by its chain hash.
pub(crate) const PUBSUB_DRAND_STR: &str = "/drand/pubsub/v0.0.0";

/// A gossiped round awaiting the listener's verdict.
pub(crate) struct Round {
    pub(crate) message_id: MessageId,
    pub(crate) source: PeerId,
    pub(crate) data: Vec<u8>,
}

/// The network service's end of the `drand` listener: rounds go in, verdicts come back.
pub struct DrandGossip {
    pub(crate) topic: TopicHash,
    pub(crate) rounds: flume::Sender<Round>,
    pub(crate) verdicts: flume::Receiver<Verdict>,
}

impl DrandGossip {
    /// `None` when the schedule has no unchained beacon, the only kind gossiped.
    pub fn new(schedule: Arc<BeaconSchedule>) -> Option<(Self, DrandGossipListener)> {
        let beacon = schedule.unchained_beacon()?;
        let topic = TopicHash::from_raw(format!("{PUBSUB_DRAND_STR}/{}", beacon.chain_hash()));
        let (rounds_tx, rounds_rx) = flume::bounded(16);
        let (verdicts_tx, verdicts_rx) = flume::unbounded();

        let gossip = Self {
            topic,
            rounds: rounds_tx,
            verdicts: verdicts_rx,
        };
        let listener = DrandGossipListener {
            schedule,
            latest: 0,
            rounds: rounds_rx,
            verdicts: verdicts_tx,
        };
        Some((gossip, listener))
    }
}

/// Verifies the rounds gossiped on the unchained beacon's topic. Only malformed payloads and bad
/// signatures are rejected; anything else unverified is ignored.
pub struct DrandGossipListener {
    schedule: Arc<BeaconSchedule>,
    /// Highest accepted round; older ones are dropped
    latest: u64,
    rounds: flume::Receiver<Round>,
    verdicts: flume::Sender<Verdict>,
}

impl DrandGossipListener {
    /// Runs until the network service drops its end.
    pub async fn run(mut self) -> anyhow::Result<()> {
        while let Ok(Round {
            message_id,
            source,
            data,
        }) = self.rounds.recv_async().await
        {
            let acceptance = self.handle_incomming_round(source, &data);
            let verdict = Verdict {
                message_id,
                source,
                acceptance,
            };
            if self.verdicts.send_async(verdict).await.is_err() {
                break;
            }
        }
        Ok(())
    }

    fn handle_incomming_round(&mut self, source: PeerId, data: &[u8]) -> MessageAcceptance {
        let verdict = |acceptance, reason| {
            metrics::record(&acceptance, reason);
            acceptance
        };

        let beacon = self
            .schedule
            .unchained_beacon()
            .expect("drand gossip requires an unchained beacon");

        let mut reader = BytesReader::from_bytes(data);
        let Ok(response) = PublicRandResponse::from_reader(&mut reader, data) else {
            debug!(%source, "drand entry could not be decoded");
            return verdict(MessageAcceptance::Reject, Reason::Malformed);
        };

        let entry = BeaconEntry::new(response.round, response.signature);
        let round = entry.round();

        if !beacon.is_epoch_round(round) {
            return verdict(MessageAcceptance::Ignore, Reason::OffEpoch);
        }
        if beacon.has_round(round) {
            return verdict(MessageAcceptance::Ignore, Reason::Duplicate);
        }
        if round <= self.latest {
            return verdict(MessageAcceptance::Ignore, Reason::Stale);
        }

        let (acceptance, reason) =
            match beacon.verify_entries(std::slice::from_ref(&entry), &BeaconEntry::default()) {
                Ok(true) => {
                    self.latest = round;
                    (MessageAcceptance::Accept, Reason::Verified)
                }
                Ok(false) => (MessageAcceptance::Reject, Reason::InvalidSignature),
                Err(_) => (MessageAcceptance::Reject, Reason::Malformed),
            };
        debug!(round, %source, ?reason, "checked drand entry from gossipsub");
        verdict(acceptance, reason)
    }
}

#[cfg(test)]
pub(crate) mod test_utils {
    use std::sync::Arc;

    use libp2p::gossipsub::MessageAcceptance;

    use crate::beacon::{BeaconPoint, BeaconSchedule, tests::fake_drand_signer::FakeDrandSigner};

    /// On the test chain, every epoch draws from a round ending in 2.
    pub(crate) const EPOCH_ROUND: u64 = 12;

    pub(crate) fn fake_drand_schedule() -> (FakeDrandSigner, Arc<BeaconSchedule>) {
        let drand = FakeDrandSigner::new();
        let beacon = drand.beacon();
        (
            drand,
            Arc::new(BeaconSchedule(vec![BeaconPoint::new(0, beacon)])),
        )
    }

    pub(crate) fn is_cached(schedule: &BeaconSchedule, round: u64) -> bool {
        schedule
            .unchained_beacon()
            .is_some_and(|beacon| beacon.has_round(round))
    }

    // `MessageAcceptance` doesn't implement `PartialEq`.
    pub(crate) fn assert_verdict(actual: &MessageAcceptance, expected: MessageAcceptance) {
        assert_eq!(
            std::mem::discriminant(actual),
            std::mem::discriminant(&expected),
            "verdict {actual:?}, expected {expected:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{
        test_utils::{EPOCH_ROUND, assert_verdict, fake_drand_schedule, is_cached},
        *,
    };
    use crate::beacon::tests::fake_drand_signer::FakeDrandSigner;
    use quickcheck_macros::quickcheck;

    fn listener(schedule: &Arc<BeaconSchedule>) -> DrandGossipListener {
        DrandGossip::new(schedule.clone())
            .expect("unchained beacon")
            .1
    }

    #[test]
    fn valid_round_is_accepted() {
        let (drand, schedule) = fake_drand_schedule();

        let verdict = listener(&schedule)
            .handle_incomming_round(PeerId::random(), &drand.encode(EPOCH_ROUND));

        assert_verdict(&verdict, MessageAcceptance::Accept);
        assert!(is_cached(&schedule, EPOCH_ROUND));
    }

    /// Entries rejected or ignored without being cached.
    #[rstest::rstest]
    #[case::undecodable(|_: &FakeDrandSigner| vec![0xff, 0xff, 0xff], MessageAcceptance::Reject)]
    #[case::round_zero(|d: &FakeDrandSigner| FakeDrandSigner::encode_raw(0, d.entry(1).signature()), MessageAcceptance::Ignore)]
    #[case::empty_signature(|_: &FakeDrandSigner| FakeDrandSigner::encode_raw(EPOCH_ROUND, &[]), MessageAcceptance::Reject)]
    #[case::wrong_length_signature(|_: &FakeDrandSigner| FakeDrandSigner::encode_raw(EPOCH_ROUND, &[1, 2, 3]), MessageAcceptance::Reject)]
    // An epoch round with the next round's signature.
    #[case::forged_signature(
        |d: &FakeDrandSigner| FakeDrandSigner::encode_raw(EPOCH_ROUND, d.entry(EPOCH_ROUND + 1).signature()),
        MessageAcceptance::Reject
    )]
    fn invalid_entry_is_not_accepted(
        #[case] payload: fn(&FakeDrandSigner) -> Vec<u8>,
        #[case] expected: MessageAcceptance,
    ) {
        let (drand, schedule) = fake_drand_schedule();

        let verdict =
            listener(&schedule).handle_incomming_round(PeerId::random(), &payload(&drand));

        assert_verdict(&verdict, expected);
        assert!(!is_cached(&schedule, EPOCH_ROUND), "nothing cached");
    }

    /// Peer bytes never panic the listener, and only a valid signature is accepted.
    #[quickcheck]
    fn arbitrary_payload_is_never_accepted(raw: Vec<u8>, round: u64, signature: Vec<u8>) {
        let (_, schedule) = fake_drand_schedule();
        let mut l = listener(&schedule);
        for payload in [raw, FakeDrandSigner::encode_raw(round, &signature)] {
            let verdict = l.handle_incomming_round(PeerId::random(), &payload);
            assert!(!matches!(verdict, MessageAcceptance::Accept), "{payload:?}");
        }
        assert!(!is_cached(&schedule, round));
    }

    /// Another relay, or an HTTP fetch, may already have cached the round.
    #[tokio::test]
    async fn cached_round_is_ignored() {
        let (drand, schedule) = fake_drand_schedule();
        let source = PeerId::random();
        let payload = drand.encode(EPOCH_ROUND);
        let forged =
            FakeDrandSigner::encode_raw(EPOCH_ROUND, drand.entry(EPOCH_ROUND + 1).signature());

        let verdict = listener(&schedule).handle_incomming_round(source, &payload);
        assert_verdict(&verdict, MessageAcceptance::Accept);

        // A fresh listener, so only the shared cache can know the round.
        let mut l = listener(&schedule);
        assert_verdict(
            &l.handle_incomming_round(source, &payload),
            MessageAcceptance::Ignore,
        );
        assert_verdict(
            &l.handle_incomming_round(source, &forged),
            MessageAcceptance::Ignore,
        );
        let beacon = schedule.unchained_beacon().expect("unchained beacon");
        assert_eq!(
            beacon.entry(EPOCH_ROUND).await.unwrap(),
            drand.entry(EPOCH_ROUND),
            "the cache keeps the valid entry"
        );
    }

    #[test]
    fn older_round_is_ignored() {
        let (drand, schedule) = fake_drand_schedule();
        let source = PeerId::random();
        let mut l = listener(&schedule);

        let verdict = l.handle_incomming_round(source, &drand.encode(EPOCH_ROUND));
        assert_verdict(&verdict, MessageAcceptance::Accept);

        let verdict = l.handle_incomming_round(source, &drand.encode(EPOCH_ROUND - 10));
        assert_verdict(&verdict, MessageAcceptance::Ignore);
        assert!(!is_cached(&schedule, EPOCH_ROUND - 10));
    }

    /// Only accepted rounds move the progression mark.
    #[test]
    fn rejected_round_does_not_advance_progression() {
        let (drand, schedule) = fake_drand_schedule();
        let source = PeerId::random();
        let mut l = listener(&schedule);
        let forged =
            FakeDrandSigner::encode_raw(EPOCH_ROUND + 10, drand.entry(EPOCH_ROUND).signature());

        let verdict = l.handle_incomming_round(source, &forged);
        assert_verdict(&verdict, MessageAcceptance::Reject);

        let verdict = l.handle_incomming_round(source, &drand.encode(EPOCH_ROUND));
        assert_verdict(&verdict, MessageAcceptance::Accept);
    }
}

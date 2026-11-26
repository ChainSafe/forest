// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::LazyLock;

use libp2p::gossipsub::MessageAcceptance;
use prometheus_client::{
    encoding::EncodeLabelSet,
    metrics::{counter::Counter, family::Family},
};

/// Why a gossiped `drand` entry got its verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub(super) enum Reason {
    Verified,
    InvalidSignature,
    Malformed,
    Stale,
    Duplicate,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct VerdictLabels {
    verdict: &'static str,
    reason: &'static str,
}

// Exported as `drand_gossip_entries_total`.
static DRAND_GOSSIP_ENTRIES: LazyLock<Family<VerdictLabels, Counter>> = LazyLock::new(|| {
    let metric = Family::default();
    crate::metrics::default_registry().register(
        "drand_gossip_entries",
        "drand beacon entries received over gossipsub, by validation verdict and reason",
        metric.clone(),
    );
    metric
});

pub(super) fn record(acceptance: &MessageAcceptance, reason: Reason) {
    let verdict = match acceptance {
        MessageAcceptance::Accept => "accept",
        MessageAcceptance::Reject => "reject",
        MessageAcceptance::Ignore => "ignore",
    };
    DRAND_GOSSIP_ENTRIES
        .get_or_create(&VerdictLabels {
            verdict,
            reason: reason.into(),
        })
        .inc();
}

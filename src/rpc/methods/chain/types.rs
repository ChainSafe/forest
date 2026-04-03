// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Eq, PartialEq, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ObjStat {
    pub size: usize,
    pub links: usize,
}
lotus_json_with_self!(ObjStat);

/// Describes how the node is currently determining finality,
/// combining probabilistic EC finality (based on observed chain health) with
/// F3 fast finality when available.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChainFinalityStatus {
    /// The shallowest epoch depth at which the
    /// probability of a chain reorganization drops below 2^-30 (~one in a
    /// billion). A value of -1 indicates the threshold was not met within the
    /// search range, which suggests degraded chain health.
    pub ec_finality_threshold_depth: i64,

    /// The most recent tipset where the reorg probability
    /// is below 2^-30, based on observed block production. Nil if the
    /// threshold is not met.
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Option<Tipset>>")]
    pub ec_finalized_tip_set: Option<Tipset>,

    /// The tipset finalized by F3 (Fast Finality), if F3
    /// is running and has issued a certificate. Nil if F3 is not available.
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Option<Tipset>>")]
    pub f3_finalized_tip_set: Option<Tipset>,

    /// The overall finalized tipset used by the node,
    /// taking the most recent of F3 and EC calculator results.
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Option<Tipset>>")]
    pub finalized_tip_set: Option<Tipset>,

    /// The current chain head used for the computation.
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Tipset>")]
    pub head: Tipset,
}
lotus_json_with_self!(ChainFinalityStatus);

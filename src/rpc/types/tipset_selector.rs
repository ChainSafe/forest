// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::chain::index::ResolveNullTipset;

#[cfg(test)]
mod tests;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct TipsetSelector {
    #[serde(
        with = "crate::lotus_json",
        skip_serializing_if = "ApiTipsetKey::is_none",
        default
    )]
    #[schemars(with = "LotusJson<TipsetKey>")]
    pub key: ApiTipsetKey,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub height: Option<TipsetHeight>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tag: Option<TipsetTag>,
}
lotus_json_with_self!(TipsetSelector);

impl TipsetSelector {
    /// Validate ensures that the [`TipsetSelector`] is valid. It checks that only one of
    /// the selection criteria is specified.
    pub fn validate(&self) -> anyhow::Result<()> {
        match (&self.key.0, &self.tag, &self.height) {
            (Some(_), None, None) | (None, Some(_), None) => {}
            (None, None, Some(height)) => {
                height.validate()?;
            }
            _ => {
                let criteria = [
                    self.key.0.is_some(),
                    self.tag.is_some(),
                    self.height.is_some(),
                ]
                .into_iter()
                .filter(|&b| b)
                .count();
                anyhow::bail!(
                    "exactly one tipset selection criteria must be specified, found: {criteria}"
                )
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct TipsetHeight {
    pub at: ChainEpoch,
    pub previous: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub anchor: Option<TipsetAnchor>,
}
lotus_json_with_self!(TipsetHeight);

impl TipsetHeight {
    /// Ensures that the [`TipsetHeight`] is valid. It checks that the height is
    /// not negative and the anchor is valid.
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.at >= 0,
            "invalid tipset height: epoch cannot be less than zero"
        );
        TipsetAnchor::validate(&self.anchor)?;
        Ok(())
    }

    pub fn resolve_null_tipset_policy(&self) -> ResolveNullTipset {
        if self.previous {
            ResolveNullTipset::TakeOlder
        } else {
            ResolveNullTipset::TakeNewer
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct TipsetAnchor {
    #[serde(
        with = "crate::lotus_json",
        skip_serializing_if = "ApiTipsetKey::is_none",
        default
    )]
    #[schemars(with = "LotusJson<TipsetKey>")]
    pub key: ApiTipsetKey,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tag: Option<TipsetTag>,
}
lotus_json_with_self!(TipsetAnchor);

impl TipsetAnchor {
    /// Validate ensures that the [`TipsetAnchor`] is valid. It checks that at most one
    /// of [`TipsetKey`] or [`TipsetTag`] is specified. Otherwise, it returns an error.
    ///
    /// Note that a [`None`] anchor is valid, and is considered to be
    /// equivalent to the default anchor, which is the tipset tagged as [`TipsetTag::Finalized`].
    pub fn validate(anchor: &Option<Self>) -> anyhow::Result<()> {
        if let Some(anchor) = anchor {
            anyhow::ensure!(
                anchor.key.0.is_none() || anchor.tag.is_none(),
                "invalid tipset anchor: at most one of key or tag must be specified"
            );
        }
        // None anchor is valid, and considered to be an equivalent to whatever the API decides the default to be.
        Ok(())
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    strum::Display,
    strum::EnumString,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    JsonSchema,
)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum TipsetTag {
    Latest,
    Finalized,
    Safe,
}

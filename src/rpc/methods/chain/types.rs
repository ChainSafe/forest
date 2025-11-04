// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Eq, PartialEq, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ObjStat {
    pub size: usize,
    pub links: usize,
}
lotus_json_with_self!(ObjStat);

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TipsetSelector {
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TipsetKey>")]
    pub key: ApiTipsetKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<TipsetHeight>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<TipsetTag>,
}
lotus_json_with_self!(TipsetSelector);

impl TipsetSelector {
    /// Validate ensures that the TipSetSelector is valid. It checks that only one of
    /// the selection criteria is specified. If no criteria are specified, it returns
    /// nil, indicating that the default selection criteria should be used as defined
    /// by the Lotus API Specification.
    pub fn validate(&self) -> anyhow::Result<()> {
        let mut criteria = 0;
        if self.key.0.is_some() {
            criteria += 1;
        }
        if self.tag.is_some() {
            criteria += 1;
        }
        if let Some(height) = &self.height {
            criteria += 1;
            height.validate()?;
        }
        if criteria != 1 {
            anyhow::bail!(
                "exactly one tipset selection criteria must be specified, found: {criteria}"
            )
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TipsetHeight {
    pub at: ChainEpoch,
    pub previous: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<TipsetAnchor>,
}
lotus_json_with_self!(TipsetHeight);

impl TipsetHeight {
    /// Ensures that the [`TipsetHeight`] is valid. It checks that the height is
    /// not negative and the anchor is valid.
    ///
    /// A zero-valued height is considered to be valid.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.at.is_negative() {
            anyhow::bail!("invalid tipset height: epoch cannot be less than zero");
        }
        if let Some(anchor) = &self.anchor {
            anchor.validate()?;
        }
        // An unspecified Anchor is valid, because it's an optional field, and falls back to whatever the API decides the default to be.
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TipsetAnchor {
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TipsetKey>")]
    pub key: ApiTipsetKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<TipsetTag>,
}
lotus_json_with_self!(TipsetAnchor);

impl TipsetAnchor {
    /// Validate ensures that the TipSetAnchor is valid. It checks that at most one
    /// of TipSetKey or TipSetTag is specified. Otherwise, it returns an error.
    ///
    /// Note that a nil or a zero-valued anchor is valid, and is considered to be
    /// equivalent to the default anchor, which is the tipset tagged as "finalized".
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.key.0.is_some() && self.tag.is_some() {
            anyhow::bail!("invalid tipset anchor: at most one of key or tag must be specified");
        }
        // Zero-valued anchor is valid, and considered to be an equivalent to whatever the API decides the default to be.
        Ok(())
    }
}

#[derive(
    Debug, Clone, Copy, strum::Display, strum::EnumString, Serialize, Deserialize, JsonSchema,
)]
#[strum(serialize_all = "lowercase")]
pub enum TipsetTag {
    Latest,
    Finalized,
    Safe,
}

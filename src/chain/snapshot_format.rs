// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::lotus_json_with_self;
use cid::Cid;
use itertools::Itertools as _;
use num::FromPrimitive as _;
use num_derive::FromPrimitive;
use nunny::Vec as NonEmpty;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, clap::ValueEnum, FromPrimitive, Clone, PartialEq, Eq, JsonSchema)]
#[repr(u64)]
pub enum FilecoinSnapshotVersion {
    V1 = 1,
    V2 = 2,
}
lotus_json_with_self!(FilecoinSnapshotVersion);

impl Serialize for FilecoinSnapshotVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u64(*self as u64)
    }
}

impl<'de> Deserialize<'de> for FilecoinSnapshotVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let i = u64::deserialize(deserializer)?;
        match FilecoinSnapshotVersion::from_u64(i) {
            Some(v) => Ok(v),
            None => Err(serde::de::Error::custom(format!(
                "invalid snapshot version {i}"
            ))),
        }
    }
}

/// Defined in <https://github.com/filecoin-project/FIPs/blob/98e33b9fa306959aa0131519eb4cc155522b2081/FRCs/frc-0108.md#snapshotmetadata>
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, derive_more::Constructor)]
#[serde(rename_all = "PascalCase")]
pub struct FilecoinSnapshotMetadata {
    /// Snapshot version
    pub version: FilecoinSnapshotVersion,
    /// Chain head tipset key
    pub head_tipset_key: NonEmpty<Cid>,
    /// F3 snapshot `CID`
    pub f3_data: Option<Cid>,
}

impl FilecoinSnapshotMetadata {
    pub fn new_v2(head_tipset_key: NonEmpty<Cid>, f3_data: Option<Cid>) -> Self {
        Self::new(FilecoinSnapshotVersion::V2, head_tipset_key, f3_data)
    }
}

impl std::fmt::Display for FilecoinSnapshotMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "Snapshot version:           {}", self.version as u64)?;
        let head_tipset_key_string = self
            .head_tipset_key
            .iter()
            .map(Cid::to_string)
            .join("\n                            ");
        writeln!(f, "Head Tipset:                {head_tipset_key_string}")?;
        write!(
            f,
            "F3 data:                    {}",
            self.f3_data
                .map(|c| c.to_string())
                .unwrap_or_else(|| "not found".into())
        )?;
        Ok(())
    }
}

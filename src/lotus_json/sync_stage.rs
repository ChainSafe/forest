// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;

use crate::chain_sync::SyncStage;

impl HasLotusJson for SyncStage {
    type LotusJson = Stringify<SyncStage>;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("idle worker"), Self::Idle)]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.into()
    }

    fn from_lotus_json(Stringify(sync_stage): Self::LotusJson) -> Self {
        sync_stage
    }
}

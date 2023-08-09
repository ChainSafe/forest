use super::*;

use crate::chain_sync::SyncStage;

#[derive(Serialize, Deserialize, From, Into)]
pub struct SyncStageLotusJson(#[serde(with = "crate::lotus_json::stringify")] SyncStage);

impl HasLotusJson for SyncStage {
    type LotusJson = SyncStageLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("idle worker"), Self::Idle)]
    }
}

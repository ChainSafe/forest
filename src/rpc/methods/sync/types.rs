// Add to src/rpc/types.rs or a similar appropriate location

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use crate::lotus_json::lotus_json_with_self;

#[derive(PartialEq, Default, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotTracker {
    pub message: String,  // The formatted progress message
}

impl SnapshotTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_message(&mut self, message: String) {
        tracing::info!("tracking snapshot message: {}", message.clone());
        self.message = message;
    }
}

lotus_json_with_self!(SnapshotTracker);
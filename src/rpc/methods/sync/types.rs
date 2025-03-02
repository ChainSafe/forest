// Add to src/rpc/types.rs or a similar appropriate location

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SnapshotTracker {
    pub message: String,  // The formatted progress message
}

impl SnapshotTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_message(&mut self, message: String) {
        self.message = message;
    }
}
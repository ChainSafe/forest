use std::collections::HashMap;
use cid::Cid;

struct MemMigrationCache {
    map: HashMap<String, Cid>
}

impl MemMigrationCache {
    fn new() -> Self {
        Self {
            map: HashMap::new()
        }
    }

    fn write(&mut self, key: &str, cid: Cid) {
        self.map.insert(key.to_string(), cid);
    }

    fn read(&self, key: &str) -> Option<&Cid> {
        self.map.get(key)
    }
}


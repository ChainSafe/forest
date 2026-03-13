// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub static DDLS: [&str; 10] = [
    r#"CREATE TABLE IF NOT EXISTS tipset_message (
		id INTEGER PRIMARY KEY,
		tipset_key_cid BLOB NOT NULL,
		height INTEGER NOT NULL,
		reverted INTEGER NOT NULL,
		message_cid BLOB,
		message_index INTEGER,
		UNIQUE (tipset_key_cid, message_cid)
	)"#,
    r#"CREATE TABLE IF NOT EXISTS eth_tx_hash (
		tx_hash TEXT PRIMARY KEY,
		message_cid BLOB NOT NULL,
		inserted_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
	)"#,
    r#"CREATE TABLE IF NOT EXISTS event (
		id INTEGER PRIMARY KEY,
		message_id INTEGER NOT NULL,
		event_index INTEGER NOT NULL,
		emitter_id INTEGER NOT NULL,
		emitter_addr BLOB,
		reverted INTEGER NOT NULL,
		FOREIGN KEY (message_id) REFERENCES tipset_message(id) ON DELETE CASCADE,
		UNIQUE (message_id, event_index)
	)"#,
    r#"CREATE TABLE IF NOT EXISTS event_entry (
		event_id INTEGER NOT NULL,
		indexed INTEGER NOT NULL,
		flags BLOB NOT NULL,
		key TEXT NOT NULL,
		codec INTEGER,
		value BLOB NOT NULL,
		FOREIGN KEY (event_id) REFERENCES event(id) ON DELETE CASCADE
	)"#,
    "CREATE INDEX IF NOT EXISTS insertion_time_index ON eth_tx_hash (inserted_at)",
    "CREATE INDEX IF NOT EXISTS idx_message_cid ON tipset_message (message_cid)",
    "CREATE INDEX IF NOT EXISTS idx_tipset_key_cid ON tipset_message (tipset_key_cid)",
    "CREATE INDEX IF NOT EXISTS idx_event_message_id ON event (message_id)",
    "CREATE INDEX IF NOT EXISTS idx_height ON tipset_message (height)",
    "CREATE INDEX IF NOT EXISTS event_entry_event_id ON event_entry(event_id)",
];

pub struct PreparedStatements {
    pub has_tipset: &'static str,
    pub is_index_empty: &'static str,
    pub has_null_round_at_height: &'static str,
    pub get_non_reverted_tipset_at_height: &'static str,
    pub count_tipsets_at_height: &'static str,
    pub get_non_reverted_tipset_message_count: &'static str,
    pub get_non_reverted_tipset_event_count: &'static str,
    pub has_reverted_events_in_tipset: &'static str,
    pub get_non_reverted_tipset_event_entries_count: &'static str,
    pub insert_eth_tx_hash: &'static str,
    pub insert_tipset_message: &'static str,
    pub update_tipset_to_non_reverted: &'static str,
    pub update_tipset_to_reverted: &'static str,
    pub update_events_to_non_reverted: &'static str,
    pub update_events_to_reverted: &'static str,
    pub get_msg_id_for_msg_cid_and_tipset: &'static str,
    pub insert_event: &'static str,
    pub insert_event_entry: &'static str,
    pub remove_tipsets_before_height: &'static str,
    pub remove_eth_hashes_older_than: &'static str,
}

impl Default for PreparedStatements {
    fn default() -> Self {
        let has_tipset = "SELECT EXISTS(SELECT 1 FROM tipset_message WHERE tipset_key_cid = ?)";
        let is_index_empty = "SELECT NOT EXISTS(SELECT 1 FROM tipset_message LIMIT 1)";
        let has_null_round_at_height =
            "SELECT NOT EXISTS(SELECT 1 FROM tipset_message WHERE height = ?)";
        let get_non_reverted_tipset_at_height =
            "SELECT tipset_key_cid FROM tipset_message WHERE height = ? AND reverted = 0 LIMIT 1";
        let count_tipsets_at_height = "SELECT COUNT(CASE WHEN reverted = 1 THEN 1 END) AS reverted_count, COUNT(CASE WHEN reverted = 0 THEN 1 END) AS non_reverted_count FROM (SELECT tipset_key_cid, MAX(reverted) AS reverted FROM tipset_message WHERE height = ? GROUP BY tipset_key_cid) AS unique_tipsets";
        let get_non_reverted_tipset_message_count = "SELECT COUNT(*) FROM tipset_message WHERE tipset_key_cid = ? AND reverted = 0 AND message_cid IS NOT NULL";
        let get_non_reverted_tipset_event_count = "SELECT COUNT(*) FROM event WHERE reverted = 0 AND message_id IN (SELECT id FROM tipset_message WHERE tipset_key_cid = ? AND reverted = 0)";
        let has_reverted_events_in_tipset = "SELECT EXISTS(SELECT 1 FROM event WHERE reverted = 1 AND message_id IN (SELECT id FROM tipset_message WHERE tipset_key_cid = ?))";
        let get_non_reverted_tipset_event_entries_count = "SELECT COUNT(ee.event_id) AS entry_count FROM event_entry ee JOIN event e ON ee.event_id = e.id JOIN tipset_message tm ON e.message_id = tm.id WHERE tm.tipset_key_cid = ? AND tm.reverted = 0";
        let insert_eth_tx_hash = "INSERT INTO eth_tx_hash (tx_hash, message_cid) VALUES (?, ?) ON CONFLICT (tx_hash) DO UPDATE SET inserted_at = CURRENT_TIMESTAMP";
        let insert_tipset_message = "INSERT INTO tipset_message (tipset_key_cid, height, reverted, message_cid, message_index) VALUES (?, ?, ?, ?, ?) ON CONFLICT (tipset_key_cid, message_cid) DO UPDATE SET reverted = 0";
        let update_tipset_to_non_reverted =
            "UPDATE tipset_message SET reverted = 0 WHERE tipset_key_cid = ?";
        let update_tipset_to_reverted =
            "UPDATE tipset_message SET reverted = 1 WHERE tipset_key_cid = ?";
        let update_events_to_non_reverted = "UPDATE event SET reverted = 0 WHERE message_id IN (SELECT id FROM tipset_message WHERE tipset_key_cid = ?)";
        let update_events_to_reverted = "UPDATE event SET reverted = 1 WHERE message_id IN (SELECT id FROM tipset_message WHERE height >= ?)";
        let get_msg_id_for_msg_cid_and_tipset = "SELECT id FROM tipset_message WHERE tipset_key_cid = ? AND message_cid = ? AND reverted = 0";
        let insert_event = "INSERT INTO event (message_id, event_index, emitter_id, emitter_addr, reverted) VALUES (?, ?, ?, ?, ?)";
        let insert_event_entry = "INSERT INTO event_entry (event_id, indexed, flags, key, codec, value) VALUES (?, ?, ?, ?, ?, ?)";
        let remove_tipsets_before_height = "DELETE FROM tipset_message WHERE height < ?";
        let remove_eth_hashes_older_than =
            "DELETE FROM eth_tx_hash WHERE inserted_at < datetime('now', ?)";

        Self {
            has_tipset,
            is_index_empty,
            has_null_round_at_height,
            get_non_reverted_tipset_at_height,
            count_tipsets_at_height,
            get_non_reverted_tipset_message_count,
            get_non_reverted_tipset_event_count,
            has_reverted_events_in_tipset,
            get_non_reverted_tipset_event_entries_count,
            insert_eth_tx_hash,
            insert_tipset_message,
            update_tipset_to_non_reverted,
            update_tipset_to_reverted,
            update_events_to_non_reverted,
            update_events_to_reverted,
            get_msg_id_for_msg_cid_and_tipset,
            insert_event,
            insert_event_entry,
            remove_tipsets_before_height,
            remove_eth_hashes_older_than,
        }
    }
}

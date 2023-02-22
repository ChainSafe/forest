// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use log::{Level, Log, Metadata, Record};
use wasm_bindgen::prelude::*;

use crate::js_ffi::console_log;

#[wasm_bindgen(inline_js = r###"
let logText = ''

function get_log_text_js() {
    return logText
}

function clear_log_js() {
    logText = ''
}

function append_log_record(record) {
    logText = `${logText}\n${record}`
}

module.exports = {
    get_log_text_js, clear_log_js, append_log_record
}
"###)]
extern "C" {
    fn append_log_record(record: String);
    fn clear_log_js();
    fn get_log_text_js() -> String;
}

#[wasm_bindgen]
pub fn clear_log() {
    clear_log_js()
}

#[wasm_bindgen]
pub fn get_log_text() -> String {
    get_log_text_js()
}

#[derive(Debug, Clone)]
pub struct JsExportableLogger {
    max_level: Level,
}

impl JsExportableLogger {
    pub const fn new(max_level: Level) -> Self {
        Self { max_level }
    }

    pub const fn max_level(&self) -> Level {
        self.max_level
    }

    fn record_to_string(record: &Record) -> String {
        let level = record.level();
        let target = record.target();
        format!("[{level}][{target}]: {}", record.args())
    }
}

impl Log for JsExportableLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.max_level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let s = Self::record_to_string(record);
            console_log(s.as_str());
            append_log_record(s);
        }
    }

    fn flush(&self) {}
}

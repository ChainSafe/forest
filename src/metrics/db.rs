// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use prometheus_client::{
    collector::Collector,
    encoding::{DescriptorEncoder, EncodeMetric},
    metrics::gauge::Gauge,
};
use std::path::PathBuf;
use tracing::error;

#[derive(Debug)]
pub struct DBCollector {
    db_directory: PathBuf,
    db_size: Gauge,
}

impl DBCollector {
    pub fn new(db_directory: PathBuf) -> Self {
        Self {
            db_directory,
            db_size: Gauge::default(),
        }
    }
}

impl Collector for DBCollector {
    fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
        let db_size = match fs_extra::dir::get_size(self.db_directory.clone()) {
            Ok(db_size) => db_size,
            Err(e) => {
                error!("Calculating DB size for metrics failed: {:?}", e);
                0
            }
        };
        self.db_size.set(db_size as _);
        let metric_encoder = encoder.encode_descriptor(
            "forest_db_size",
            "Size of Forest database in bytes",
            // Using Some(&Unit::Bytes) here changes the output to
            // # HELP forest_db_size_bytes Size of Forest database in bytes
            // # TYPE forest_db_size_bytes gauge
            // # UNIT forest_db_size_bytes bytes
            // forest_db_size_bytes 9281452850
            None,
            self.db_size.metric_type(),
        )?;
        self.db_size.encode(metric_encoder)?;
        Ok(())
    }
}

use std::os::unix::fs::MetadataExt;

use prometheus::core::{Collector, Desc};
use prometheus::proto;
use prometheus::{Gauge, Opts};

pub struct DBCollector {
    db_directory: String,
    descs: Vec<Desc>,
    db_size: Gauge,
}

impl DBCollector {
    pub fn new(db_directory: String) -> Self {
        let mut descs: Vec<Desc> = vec![];
        let db_size = Gauge::with_opts(Opts::new(
            "forest_db_size",
            "Size of Forest database in bytes",
        ))
        .expect("Creating forest_db_size gauge must succeed");
        descs.extend(db_size.desc().into_iter().cloned());
        Self {
            db_directory,
            descs,
            db_size,
        }
    }
}

impl Collector for DBCollector {
    fn desc(&self) -> Vec<&Desc> {
        self.descs.iter().collect()
    }

    fn collect(&self) -> Vec<proto::MetricFamily> {
        let metadata = std::fs::metadata(self.db_directory.clone()).expect(
            "Retrieving metadata for the database directory from the filesystem must succeed",
        );
        self.db_size.set(metadata.size() as f64);

        let mut metric_families = vec![];
        metric_families.extend(self.db_size.collect());
        metric_families
    }
}

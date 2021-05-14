use prometheus::{
    core::{AtomicU64, GenericCounter, GenericCounterVec, Opts},
    Error as PrometheusError, Histogram, HistogramOpts, Registry,
};

#[derive(Clone)]
pub struct Metrics {
    pub tipset_processing_time: Box<Histogram>,
    pub gossipsub_message_total: Box<GenericCounterVec<AtomicU64>>,
    pub invalid_tipset_total: Box<GenericCounter<AtomicU64>>,
    pub tipset_range_sync_failure_total: Box<GenericCounter<AtomicU64>>,
}

impl Metrics {
    pub fn register(registry: &Registry) -> Result<Self, PrometheusError> {
        let tipset_processing_time = Box::new(Histogram::with_opts(HistogramOpts {
            common_opts: Opts::new(
                "tipset_processing_time",
                "Duration of routine which processes Tipsets to include them in the store",
            ),
            buckets: vec![],
        })?);
        let gossipsub_message_total = Box::new(GenericCounterVec::<AtomicU64>::new(
            Opts::new(
                "gossipsub_messsage_total",
                "Total number of gossipsub message by type",
            ),
            &[
                "hello_request",
                "peer_connected",
                "peer_disconnected",
                "pubsub_message_block",
                "pubsub_message_message",
                "chain_exchange_request",
                "bitswap_block",
            ],
        )?);
        let invalid_tipset_total = Box::new(GenericCounter::<AtomicU64>::new(
            "invalid_tipset_total",
            "Total number of invalid tipsets received over gossipsub",
        )?);
        let tipset_range_sync_failure_total = Box::new(GenericCounter::<AtomicU64>::new(
            "tipset_range_sync_failure_total",
            "Total number of errors produced by TipsetRangeSyncers",
        )?);

        registry.register(tipset_processing_time.clone())?;
        registry.register(gossipsub_message_total.clone())?;
        registry.register(invalid_tipset_total.clone())?;
        registry.register(tipset_range_sync_failure_total.clone())?;

        Ok(Self {
            tipset_processing_time,
            gossipsub_message_total,
            invalid_tipset_total,
            tipset_range_sync_failure_total,
        })
    }
}

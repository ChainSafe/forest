// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::chain_sync::SyncConfig;
use crate::db::db_engine::DbConfig;
use crate::libp2p::Libp2pConfig;
use crate::networks::ChainConfig;
use core::time::Duration;
use serde::{
    de::{DeserializeSeed, EnumAccess, Error, Unexpected, VariantAccess, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{fmt, path::PathBuf, str::FromStr, sync::Arc};
use tracing::log::LevelFilter;

use super::client::Client;

static LOG_LEVEL_NAMES: [&str; 6] = ["OFF", "ERROR", "WARN", "INFO", "DEBUG", "TRACE"];

#[derive(PartialEq, Eq, Debug, Clone, Hash)]
pub struct LogLevelFilter(LevelFilter);

// ported from log crate
impl Serialize for LogLevelFilter {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0 {
            LevelFilter::Off => serializer.serialize_unit_variant("LevelFilter", 0, "OFF"),
            LevelFilter::Error => serializer.serialize_unit_variant("LevelFilter", 1, "ERROR"),
            LevelFilter::Warn => serializer.serialize_unit_variant("LevelFilter", 2, "WARN"),
            LevelFilter::Info => serializer.serialize_unit_variant("LevelFilter", 3, "INFO"),
            LevelFilter::Debug => serializer.serialize_unit_variant("LevelFilter", 4, "DEBUG"),
            LevelFilter::Trace => serializer.serialize_unit_variant("LevelFilter", 5, "TRACE"),
        }
    }
}

impl<'de> Deserialize<'de> for LogLevelFilter {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LevelFilterIdentifier;

        impl<'de> Visitor<'de> for LevelFilterIdentifier {
            type Value = LevelFilter;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("log level filter")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                // Case insensitive.
                FromStr::from_str(s).map_err(|_| Error::unknown_variant(s, &LOG_LEVEL_NAMES))
            }

            fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
            where
                E: Error,
            {
                let variant = std::str::from_utf8(value)
                    .map_err(|_| Error::invalid_value(Unexpected::Bytes(value), &self))?;

                self.visit_str(variant)
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                let variant = LOG_LEVEL_NAMES
                    .get(v as usize)
                    .ok_or_else(|| Error::invalid_value(Unexpected::Unsigned(v), &self))?;

                self.visit_str(variant)
            }
        }

        impl<'de> DeserializeSeed<'de> for LevelFilterIdentifier {
            type Value = LevelFilter;

            fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserializer.deserialize_identifier(LevelFilterIdentifier)
            }
        }

        struct LevelFilterEnum;

        impl<'de> Visitor<'de> for LevelFilterEnum {
            type Value = LevelFilter;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("log level filter")
            }

            fn visit_enum<A>(self, value: A) -> Result<Self::Value, A::Error>
            where
                A: EnumAccess<'de>,
            {
                let (level_filter, variant) = value.variant_seed(LevelFilterIdentifier)?;
                // Every variant is a unit variant.
                variant.unit_variant()?;
                Ok(level_filter)
            }
        }

        let a = deserializer
            .deserialize_enum("LevelFilter", &LOG_LEVEL_NAMES, LevelFilterEnum)
            .map(LogLevelFilter);

        a
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
pub struct LogConfig {
    pub filters: Vec<LogValue>,
}

impl LogConfig {
    pub(in crate::cli_shared) fn to_filter_string(&self) -> String {
        self.filters
            .iter()
            .map(|f| format!("{}={}", f.module, f.level.0))
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            filters: vec![
                LogValue::new("axum", LogLevelFilter(LevelFilter::Warn)),
                LogValue::new(
                    "bellperson::groth16::aggregate::verify",
                    LogLevelFilter(LevelFilter::Warn),
                ),
                LogValue::new("filecoin_proofs", LogLevelFilter(LevelFilter::Warn)),
                LogValue::new("libp2p_bitswap", LogLevelFilter(LevelFilter::Off)),
                LogValue::new("libp2p_gossipsub", LogLevelFilter(LevelFilter::Error)),
                LogValue::new("libp2p_kad", LogLevelFilter(LevelFilter::Error)),
                LogValue::new("rpc", LogLevelFilter(LevelFilter::Error)),
                LogValue::new("storage_proofs_core", LogLevelFilter(LevelFilter::Warn)),
                LogValue::new("tracing_loki", LogLevelFilter(LevelFilter::Off)),
            ],
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone)]
pub struct LogValue {
    pub module: String,
    pub level: LogLevelFilter,
}

impl LogValue {
    pub fn new(module: &str, level: LogLevelFilter) -> Self {
        Self {
            module: module.to_string(),
            level,
        }
    }
}

/// Structure that defines daemon configuration when process is detached
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct DaemonConfig {
    pub user: Option<String>,
    pub group: Option<String>,
    pub umask: u16,
    pub stdout: PathBuf,
    pub stderr: PathBuf,
    pub work_dir: PathBuf,
    pub pid_file: Option<PathBuf>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            user: None,
            group: None,
            umask: 0o027,
            stdout: "forest.out".into(),
            stderr: "forest.err".into(),
            work_dir: ".".into(),
            pid_file: None,
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Default, Debug)]
pub struct TokioConfig {
    pub worker_threads: Option<usize>,
    pub max_blocking_threads: Option<usize>,
    pub thread_keep_alive: Option<Duration>,
    pub thread_stack_size: Option<usize>,
    pub global_queue_interval: Option<u32>,
}

#[derive(Serialize, Deserialize, PartialEq, Default, Debug, Clone)]
#[serde(default)]
pub struct Config {
    pub client: Client,
    pub parity_db: crate::db::parity_db_config::ParityDbConfig,
    pub network: Libp2pConfig,
    pub sync: SyncConfig,
    pub chain: Arc<ChainConfig>,
    pub daemon: DaemonConfig,
    pub log: LogConfig,
    pub tokio: TokioConfig,
}

impl Config {
    pub fn db_config(&self) -> &DbConfig {
        &self.parity_db
    }
}

#[cfg(test)]
mod test {
    use std::{
        net::{Ipv4Addr, SocketAddr},
        path::PathBuf,
    };

    use crate::utils::io::ProgressBarVisibility;
    use chrono::Duration;
    use quickcheck::Arbitrary;
    use quickcheck_macros::quickcheck;
    use tracing_subscriber::EnvFilter;

    use super::*;

    /// Partial configuration, as some parts of the proper one don't implement
    /// required traits (i.e. Debug)
    // This should be removed in #2965
    #[derive(Clone, Debug)]
    struct ConfigPartial {
        client: Client,
        parity_db: crate::db::parity_db_config::ParityDbConfig,
        network: crate::libp2p::Libp2pConfig,
        sync: crate::chain_sync::SyncConfig,
    }

    impl From<ConfigPartial> for Config {
        fn from(val: ConfigPartial) -> Self {
            Config {
                client: val.client,
                parity_db: val.parity_db,
                network: val.network,
                sync: val.sync,
                chain: Arc::new(ChainConfig::default()),
                daemon: DaemonConfig::default(),
                log: Default::default(),
                tokio: Default::default(),
            }
        }
    }

    impl Arbitrary for ConfigPartial {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            ConfigPartial {
                client: Client {
                    data_dir: PathBuf::arbitrary(g),
                    genesis_file: Option::arbitrary(g),
                    enable_rpc: bool::arbitrary(g),
                    rpc_token: Option::arbitrary(g),
                    snapshot: bool::arbitrary(g),
                    snapshot_height: Option::arbitrary(g),
                    snapshot_path: Option::arbitrary(g),
                    snapshot_head: Option::arbitrary(g),
                    skip_load: bool::arbitrary(g),
                    encrypt_keystore: bool::arbitrary(g),
                    metrics_address: SocketAddr::arbitrary(g),
                    rpc_address: SocketAddr::arbitrary(g),
                    token_exp: Duration::milliseconds(i64::arbitrary(g)),
                    show_progress_bars: ProgressBarVisibility::arbitrary(g),
                },
                parity_db: crate::db::parity_db_config::ParityDbConfig {
                    enable_statistics: bool::arbitrary(g),
                    compression_type: String::arbitrary(g),
                },
                network: Libp2pConfig {
                    listening_multiaddrs: vec![Ipv4Addr::arbitrary(g).into()],
                    bootstrap_peers: vec![Ipv4Addr::arbitrary(g).into(); u8::arbitrary(g) as usize],
                    mdns: bool::arbitrary(g),
                    kademlia: bool::arbitrary(g),
                    target_peer_count: u32::arbitrary(g),
                },
                sync: SyncConfig {
                    req_window: i64::arbitrary(g),
                    tipset_sample_size: u32::arbitrary(g) as _,
                },
            }
        }
    }

    #[quickcheck]
    fn test_config_all_params_under_section(config: ConfigPartial) {
        let config = Config::from(config);
        let serialized_config =
            toml::to_string(&config).expect("could not serialize the configuration");
        assert_eq!(
            serialized_config
                .trim_start()
                .chars()
                .next()
                .expect("configuration empty"),
            '['
        )
    }

    #[test]
    fn test_default_log_filters() {
        let config = LogConfig::default();
        EnvFilter::builder()
            .parse(config.to_filter_string())
            .unwrap();
    }
}

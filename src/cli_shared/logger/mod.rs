// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use tracing_chrome::{ChromeLayerBuilder, FlushGuard};
use tracing_subscriber::{filter::LevelFilter, prelude::*, EnvFilter};

use crate::cli_shared::cli::CliOpts;

pub fn setup_logger(opts: &CliOpts) -> (Option<tracing_loki::BackgroundTask>, Option<FlushGuard>) {
    let mut loki_task = None;
    let tracing_tokio_console = if opts.tokio_console {
        Some(
            console_subscriber::ConsoleLayer::builder()
                .with_default_env()
                .spawn(),
        )
    } else {
        None
    };
    let tracing_loki = if opts.loki {
        let (layer, task) = tracing_loki::layer(
            tracing_loki::url::Url::parse(&opts.loki_endpoint)
                .map_err(|e| format!("Unable to parse loki endpoint {}: {e}", &opts.loki_endpoint))
                .unwrap(),
            vec![(
                "host".into(),
                gethostname::gethostname()
                    .to_str()
                    .unwrap_or_default()
                    .into(),
            )]
            .into_iter()
            .collect(),
            Default::default(),
        )
        .map_err(|e| format!("Unable to create loki layer: {e}"))
        .unwrap();
        loki_task = Some(task);
        Some(layer.with_filter(LevelFilter::DEBUG))
    } else {
        None
    };
    let tracing_rolling_file = if let Some(log_dir) = &opts.log_dir {
        let file_appender = tracing_appender::rolling::hourly(log_dir, "forest.log");
        Some(
            tracing_subscriber::fmt::Layer::new()
                .with_ansi(false)
                .with_writer(file_appender)
                .with_filter(get_env_filter()),
        )
    } else {
        None
    };

    // Go to <https://ui.perfetto.dev> to browse trace files.
    // You may want to call ChromeLayerBuilder::trace_style as appropriate
    let (chrome_layer, flush_guard) =
        match std::env::var_os("CHROME_TRACE_FILE").map(|path| match path.is_empty() {
            true => ChromeLayerBuilder::new().build(),
            false => ChromeLayerBuilder::new().file(path).build(),
        }) {
            Some((a, b)) => (Some(a), Some(b)),
            None => (None, None),
        };

    tracing_subscriber::registry()
        .with(tracing_tokio_console)
        .with(tracing_loki)
        .with(tracing_rolling_file)
        .with(chrome_layer)
        .with(
            tracing_subscriber::fmt::Layer::new()
                .with_ansi(opts.color.coloring_enabled())
                .with_filter(get_env_filter()),
        )
        .init();
    (loki_task, flush_guard)
}

/// Returns an [`EnvFilter`] according to the `RUST_LOG` environment variable, or a default
/// - see [`default_env_filter`]
///
/// Note that [`tracing_subscriber::filter::Builder`] only allows a single default directive,
/// whereas we want to provide multiple.
/// See also <https://github.com/tokio-rs/tracing/blob/27f688efb72316a26f3ec1f952c82626692c08ff/tracing-subscriber/src/filter/env/builder.rs#L189-L194>
fn get_env_filter() -> EnvFilter {
    use std::env::{
        self,
        VarError::{NotPresent, NotUnicode},
    };
    match env::var(tracing_subscriber::EnvFilter::DEFAULT_ENV) {
        Ok(s) => EnvFilter::new(s),
        Err(NotPresent) => default_env_filter(),
        Err(NotUnicode(_)) => EnvFilter::default(),
    }
}

fn default_env_filter() -> EnvFilter {
    let default_directives = [
        "info",
        "bellperson::groth16::aggregate::verify=warn",
        "axum=warn",
        "filecoin_proofs=warn",
        "libp2p_bitswap=off",
        "libp2p_gossipsub=error",
        "libp2p_kad=error",
        "rpc=error",
        "storage_proofs_core=warn",
        "tracing_loki=off",
    ];
    EnvFilter::try_new(default_directives.join(",")).unwrap()
}

#[test]
fn test_default_env_filter() {
    let _did_not_panic = default_env_filter();
}

// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::pin::Pin;

use futures::Future;
use tracing_subscriber::{EnvFilter, Registry, prelude::*};

use crate::cli_shared::cli::CliOpts;
use crate::utils::misc::LoggingColor;

type BackgroundTask = Pin<Box<dyn Future<Output = ()> + Send>>;

#[derive(Default)]
pub struct Guards {
    #[cfg(feature = "tracing-chrome")]
    tracing_chrome: Option<tracing_chrome::FlushGuard>,
}

#[allow(unused_mut)]
pub fn setup_logger(opts: &CliOpts) -> (Vec<BackgroundTask>, Guards) {
    let mut background_tasks: Vec<BackgroundTask> = vec![];
    let mut guards = Guards::default();
    let mut layers: Vec<Box<dyn tracing_subscriber::layer::Layer<Registry> + Send + Sync>> =
        // console logger
        vec![Box::new(
            tracing_subscriber::fmt::Layer::new()
                .with_ansi(opts.color.coloring_enabled())
                .with_filter(get_env_filter(default_env_filter())),
        )];

    // file logger
    if let Some(log_dir) = &opts.log_dir {
        let file_appender = tracing_appender::rolling::hourly(log_dir, "forest.log");
        layers.push(Box::new(
            tracing_subscriber::fmt::Layer::new()
                .with_ansi(false)
                .with_writer(file_appender)
                .with_filter(get_env_filter(default_env_filter())),
        ));
    }

    if opts.tokio_console {
        #[cfg(not(feature = "tokio-console"))]
        tracing::warn!(
            "`tokio-console` is unavailable, forest binaries need to be recompiled with `tokio-console` feature"
        );

        #[cfg(feature = "tokio-console")]
        layers.push(Box::new(
            console_subscriber::ConsoleLayer::builder()
                .with_default_env()
                .spawn(),
        ));
    }

    if opts.loki {
        #[cfg(not(feature = "tracing-loki"))]
        tracing::warn!(
            "`tracing-loki` is unavailable, forest binaries need to be recompiled with `tracing-loki` feature"
        );

        #[cfg(feature = "tracing-loki")]
        {
            let (layer, task) = tracing_loki::layer(
                tracing_loki::url::Url::parse(&opts.loki_endpoint)
                    .map_err(|e| {
                        format!("Unable to parse loki endpoint {}: {e}", &opts.loki_endpoint)
                    })
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
            background_tasks.push(Box::pin(task));
            layers.push(Box::new(
                layer.with_filter(tracing_subscriber::filter::LevelFilter::INFO),
            ));
        }
    }

    // Go to <https://ui.perfetto.dev> to browse trace files.
    // You may want to call ChromeLayerBuilder::trace_style as appropriate
    if let Some(_chrome_trace_file) = std::env::var_os("CHROME_TRACE_FILE") {
        #[cfg(not(feature = "tracing-chrome"))]
        tracing::warn!(
            "`tracing-chrome` is unavailable, forest binaries need to be recompiled with `tracing-chrome` feature"
        );

        #[cfg(feature = "tracing-chrome")]
        {
            let (layer, guard) = match _chrome_trace_file.is_empty() {
                true => tracing_chrome::ChromeLayerBuilder::new().build(),
                false => tracing_chrome::ChromeLayerBuilder::new()
                    .file(_chrome_trace_file)
                    .build(),
            };

            guards.tracing_chrome = Some(guard);
            layers.push(Box::new(layer));
        }
    }

    tracing_subscriber::registry().with(layers).init();
    (background_tasks, guards)
}

// Log warnings to stderr
pub fn setup_minimal_logger() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::Layer::new()
                .with_ansi(LoggingColor::Auto.coloring_enabled())
                .with_writer(std::io::stderr)
                .with_filter(get_env_filter(default_tool_filter())),
        )
        .init();
}

/// Returns an [`EnvFilter`] according to the `RUST_LOG` environment variable, or a default
/// - see [`default_env_filter`] and [`default_tool_filter`]
///
/// Note that [`tracing_subscriber::filter::Builder`] only allows a single default directive,
/// whereas we want to provide multiple.
/// See also <https://github.com/tokio-rs/tracing/blob/27f688efb72316a26f3ec1f952c82626692c08ff/tracing-subscriber/src/filter/env/builder.rs#L189-L194>
fn get_env_filter(def: EnvFilter) -> EnvFilter {
    use std::env::{
        self,
        VarError::{NotPresent, NotUnicode},
    };
    match env::var(tracing_subscriber::EnvFilter::DEFAULT_ENV) {
        Ok(s) => EnvFilter::new(s),
        Err(NotPresent) => def,
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
        "storage_proofs_core=warn",
        "tracing_loki=off",
        "quinn_udp=error",
    ];
    EnvFilter::try_new(default_directives.join(",")).unwrap()
}

fn default_tool_filter() -> EnvFilter {
    let default_directives = [
        "info",
        "bellperson::groth16::aggregate::verify=warn",
        "storage_proofs_core=warn",
        "axum=warn",
        "filecoin_proofs=warn",
        "forest::snapshot=info",
        "forest::progress=info",
        "libp2p_bitswap=off",
        "tracing_loki=off",
        "hickory_resolver::hosts=off",
        "libp2p_swarm=off",
    ];
    EnvFilter::try_new(default_directives.join(",")).unwrap()
}

#[test]
fn test_default_env_filter() {
    let _did_not_panic = default_env_filter();
}

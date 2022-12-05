// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::cli::{CliOpts, LogConfig};
use atty::Stream;
use pretty_env_logger::env_logger::WriteStyle;
use std::str::FromStr;
use tracing_subscriber::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoggingColor {
    Always,
    Auto,
    Never,
}

impl FromStr for LoggingColor {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(LoggingColor::Auto),
            "always" => Ok(LoggingColor::Always),
            "never" => Ok(LoggingColor::Never),
            _ => Err(Self::Err::msg(
                "Invalid logging color output. Must be one of Auto, Always, Never",
            )),
        }
    }
}

impl From<LoggingColor> for WriteStyle {
    fn from(color: LoggingColor) -> WriteStyle {
        match color {
            LoggingColor::Always => WriteStyle::Always,
            LoggingColor::Auto => {
                if atty::is(Stream::Stdout) {
                    WriteStyle::Always
                } else {
                    WriteStyle::Never
                }
            }
            LoggingColor::Never => WriteStyle::Never,
        }
    }
}

pub fn setup_logger(
    log_config: &LogConfig,
    opts: &CliOpts,
) -> (Option<tracing_loki::BackgroundTask>,) {
    let env_filter = tracing_subscriber::filter::EnvFilter::builder().parse_lossy(
        [
            "info".into(),
            {
                let filters: Vec<_> = log_config
                    .filters
                    .iter()
                    .map(|f| format!("{}={}", f.module, f.level))
                    .collect();
                filters.join(",")
            },
            std::env::var(tracing_subscriber::filter::EnvFilter::DEFAULT_ENV).unwrap_or_default(),
        ]
        .join(","),
    );

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
        Some(layer.with_filter(tracing_subscriber::filter::LevelFilter::TRACE))
    } else {
        None
    };

    tracing_subscriber::registry()
        .with(tracing_tokio_console)
        .with(tracing_loki)
        .with(
            tracing_subscriber::fmt::Layer::new()
                .with_ansi(opts.color != LoggingColor::Never)
                .with_filter(env_filter),
        )
        .init();
    (loki_task,)
}

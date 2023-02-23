// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::io::Write;

use anes::execute;
use clap::Parser;
use forest_cli_shared::cli::{CliOpts, FOREST_VERSION_STRING, HELP_MESSAGE};
use tokio::{signal, task};

/// CLI structure generated when interacting with Forest binary
#[derive(Parser)]
#[command(name = env!("CARGO_PKG_NAME"), author = env!("CARGO_PKG_AUTHORS"), version = FOREST_VERSION_STRING.as_str(), about = env!("CARGO_PKG_DESCRIPTION"))]
#[command(help_template(HELP_MESSAGE))]
pub struct Cli {
    #[clap(flatten)]
    pub opts: CliOpts,
    pub cmd: Option<String>,
}

pub fn set_sigint_handler() {
    task::spawn(async {
        let _ = signal::ctrl_c().await;

        // the cursor can go missing if we hit ctrl-c during a prompt, so we always
        // restore it
        let mut stdout = std::io::stdout();
        #[allow(clippy::question_mark)]
        execute!(&mut stdout, anes::ShowCursor).unwrap();
    });
}

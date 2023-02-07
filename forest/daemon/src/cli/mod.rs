// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    cell::RefCell,
    io::Write,
    process,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use anes::execute;
use forest_cli_shared::cli::{CliOpts, FOREST_VERSION_STRING};
use futures::channel::oneshot::Receiver;
use log::{info, warn};
use structopt::StructOpt;

/// CLI structure generated when interacting with Forest binary
#[derive(StructOpt)]
#[structopt(
    name = env!("CARGO_PKG_NAME"),
    version = FOREST_VERSION_STRING.as_str(),
    about = env!("CARGO_PKG_DESCRIPTION"),
    author = env!("CARGO_PKG_AUTHORS")
)]
pub struct Cli {
    #[structopt(flatten)]
    pub opts: CliOpts,
    pub cmd: Option<String>,
}

pub fn set_sigint_handler() -> Receiver<()> {
    let (ctrlc_send, ctrlc_oneshot) = futures::channel::oneshot::channel();
    let ctrlc_send_c = RefCell::new(Some(ctrlc_send));

    let running = Arc::new(AtomicUsize::new(0));
    ctrlc::set_handler(move || {
        let prev = running.fetch_add(1, Ordering::SeqCst);
        if prev == 0 {
            warn!("Got interrupt, shutting down...");
            let mut stdout = std::io::stdout();
            #[allow(clippy::question_mark)]
            execute!(&mut stdout, anes::ShowCursor).unwrap();
            // Send sig int in channel to blocking task
            if let Some(ctrlc_send) = ctrlc_send_c.try_borrow_mut().unwrap().take() {
                ctrlc_send.send(()).expect("Error sending ctrl-c message");
            }
        } else {
            info!("Exiting process");
            process::exit(0);
        }
    })
    .expect("Error setting Ctrl-C handler");

    ctrlc_oneshot
}

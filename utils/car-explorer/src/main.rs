// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::{cell::RefCell, path::PathBuf, rc::Rc};

use anyhow::bail;
use cid::Cid;
use clap::Parser;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_car::load_car;
use tokio_util::compat::TokioAsyncReadCompatExt;

/// CLI options for loading a CAR file
#[derive(Parser, Debug)]
#[command(name = env!("CARGO_PKG_NAME"), author = env!("CARGO_PKG_AUTHORS"), version = env!("CARGO_PKG_VERSION"), about = env!("CARGO_PKG_DESCRIPTION"))]
struct Cli {
    /// Path to CAR file
    path: PathBuf,
}

struct KeyReader {
    keys: Rc<RefCell<Vec<Cid>>>,
}

impl KeyReader {
    fn new() -> Self {
        Self {
            keys: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn keys(&self) -> Vec<Cid> {
        self.keys.borrow().clone()
    }
}

impl Blockstore for KeyReader {
    fn get(&self, _k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        bail!("KeyReader does not support get")
    }

    fn put_keyed(&self, k: &Cid, _block: &[u8]) -> anyhow::Result<()> {
        self.keys.borrow_mut().push(*k);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Capture Cli inputs
    let Cli { path } = Cli::parse();

    let file = tokio::fs::File::open(path).await?;
    let reader = tokio::io::BufReader::new(file);
    let key_reader = KeyReader::new();
    load_car(&key_reader, reader.compat()).await?;

    key_reader.keys().iter().for_each(|k| println!("{}", k));

    Ok(())
}

// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use clap::Subcommand;
use fvm_ipld_car::Block;
use fvm_ipld_car::CarHeader;
use fvm_ipld_car::CarReader;
use tokio_util::compat::TokioAsyncReadCompatExt;

use crate::ipld::CidHashSet;

#[derive(Debug, Subcommand)]
pub enum CarCommands {
    Concat {
        first: PathBuf,
        second: PathBuf,
        /// The output `.car` file path
        #[arg(short, long)]
        output: PathBuf,
    },
}

impl CarCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Concat {
                first,
                second,
                output,
            } => {
                let reader_a = CarReader::new(
                    tokio::io::BufReader::new(tokio::fs::File::open(first).await?).compat(),
                )
                .await?;
                let reader_b = CarReader::new(
                    tokio::io::BufReader::new(tokio::fs::File::open(second).await?).compat(),
                )
                .await?;
                let mut roots = vec![];
                {
                    let mut seen = CidHashSet::default();
                    for header in [&reader_a.header, &reader_b.header] {
                        for &root in &header.roots {
                            if seen.insert(root) {
                                println!("roots.push {root}");
                                roots.push(root);
                            }
                        }
                    }
                }

                let (tx, rx) = flume::bounded(100);

                let write_task = tokio::spawn(async move {
                    let car_writer = CarHeader::from(roots);
                    let mut output_file =
                        tokio::io::BufWriter::new(tokio::fs::File::create(output).await?).compat();
                    let mut stream = rx.stream();
                    car_writer
                        .write_stream_async(&mut output_file, &mut stream)
                        .await?;

                    Ok::<_, anyhow::Error>(())
                });

                let mut seen = CidHashSet::default();
                for mut reader in [reader_a, reader_b] {
                    while let Some(Block { cid, data }) = reader.next_block().await? {
                        if seen.insert(cid) {
                            tx.send_async((cid, data)).await?;
                        }
                    }
                }

                drop(tx);
                write_task.await??;
            }
        }
        Ok(())
    }
}

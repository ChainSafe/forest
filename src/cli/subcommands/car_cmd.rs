// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use clap::Subcommand;
use futures::{AsyncRead, StreamExt, TryStreamExt};
use fvm_ipld_car::Block;
use fvm_ipld_car::CarHeader;
use fvm_ipld_car::CarReader;
use tokio_util::compat::TokioAsyncReadCompatExt;

use crate::ipld::CidHashSet;

#[derive(Debug, Subcommand)]
pub enum CarCommands {
    Concat {
        /// A list of `.car` file paths
        car_files: Vec<PathBuf>,
        /// The output `.car` file path
        #[arg(short, long)]
        output: PathBuf,
    },
}

impl CarCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Concat { car_files, output } => {
                let readers: Vec<_> = futures::stream::iter(car_files)
                    .then(|f| async {
                        CarReader::new(
                            tokio::io::BufReader::new(tokio::fs::File::open(f).await?).compat(),
                        )
                        .await
                    })
                    .try_collect()
                    .await?;
                let mut roots = vec![];
                {
                    let mut seen = CidHashSet::default();
                    for reader in &readers {
                        for &root in &reader.header.roots {
                            if seen.insert(root) {
                                println!("roots.push {root}");
                                roots.push(root);
                            }
                        }
                    }
                }

                let mut stream = Box::pin(
                    futures::stream::unfold(
                        MultiCarDedupReader::new(readers),
                        move |mut reader| async {
                            reader
                                .next_block()
                                .await
                                .expect("Failed calling `MultiCarDedupReader::next_block`")
                                .map(|b| (b, reader))
                        },
                    )
                    .map(|out| (out.cid, out.data)),
                );

                let car_writer = CarHeader::from(roots);
                let mut output_file =
                    tokio::io::BufWriter::new(tokio::fs::File::create(output).await?).compat();
                car_writer
                    .write_stream_async(&mut output_file, &mut stream)
                    .await?;
            }
        }
        Ok(())
    }
}

struct MultiCarDedupReader<R>
where
    R: AsyncRead + Send + Unpin,
{
    readers: Vec<CarReader<R>>,
    index: usize,
    seen: CidHashSet,
}

impl<R> MultiCarDedupReader<R>
where
    R: AsyncRead + Send + Unpin,
{
    fn new(readers: Vec<CarReader<R>>) -> Self {
        Self {
            readers,
            index: 0,
            seen: Default::default(),
        }
    }

    // Note: Using loop instead of recursion here to avoid stack overflow
    async fn next_block(&mut self) -> Result<Option<Block>, fvm_ipld_car::Error> {
        loop {
            if self.index >= self.readers.len() {
                break Ok(None);
            } else if let Some(block) = self.readers[self.index].next_block().await? {
                if self.seen.insert(block.cid) {
                    break Ok(Some(block));
                }
            } else {
                self.index += 1;
            };
        }
    }
}

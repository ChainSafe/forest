// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use async_recursion::async_recursion;
use clap::Subcommand;
use futures::{AsyncRead, StreamExt};
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
                let mut readers = Vec::with_capacity(car_files.len());
                for f in car_files {
                    readers.push(
                        CarReader::new(
                            tokio::io::BufReader::new(tokio::fs::File::open(f).await?).compat(),
                        )
                        .await?,
                    );
                }
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

    #[async_recursion]
    async fn next_block(&mut self) -> Result<Option<Block>, fvm_ipld_car::Error> {
        while let Some(block) = if self.index >= self.readers.len() {
            Ok(None)
        } else if let Some(block) = self.readers[self.index].next_block().await? {
            // Note: Using while loop here because below code causes stack overflow in unit tests
            // ```rust
            // if self.seen.insert(block.cid) {
            //  Ok(Some(block))
            // } else {
            //     self.next_block().await
            // }
            // ```
            Ok(Some(block))
        } else {
            self.index += 1;
            self.next_block().await
        }? {
            if self.seen.insert(block.cid) {
                return Ok(Some(block));
            }
        }

        Ok(None)
    }
}

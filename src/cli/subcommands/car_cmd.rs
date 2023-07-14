// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use async_trait::async_trait;
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

#[async_trait]
trait CarBlockProvider {
    async fn next_block(&mut self) -> Result<Option<Block>, fvm_ipld_car::Error>;
}

#[async_trait]
impl<R> CarBlockProvider for CarReader<R>
where
    R: AsyncRead + Send + Unpin,
{
    async fn next_block(&mut self) -> Result<Option<Block>, fvm_ipld_car::Error> {
        CarReader::next_block(self).await
    }
}

struct MultiCarDedupReader<R: CarBlockProvider> {
    readers: Vec<R>,
    index: usize,
    seen: CidHashSet,
}

impl<R: CarBlockProvider> MultiCarDedupReader<R> {
    fn new(readers: Vec<R>) -> Self {
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

#[cfg(test)]
mod tests {
    use std::vec::IntoIter;

    use super::*;
    use ahash::HashSet;
    use cid::multihash;
    use cid::multihash::MultihashDigest;
    use cid::Cid;
    use pretty_assertions::assert_eq;
    use quickcheck::Arbitrary;
    use quickcheck_macros::quickcheck;

    #[async_trait]
    impl CarBlockProvider for IntoIter<Block> {
        async fn next_block(&mut self) -> Result<Option<Block>, fvm_ipld_car::Error> {
            Ok(self.next())
        }
    }

    #[derive(Debug, Clone)]
    struct Blocks(Vec<Block>);

    impl Arbitrary for Blocks {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let n = u16::arbitrary(g) as usize;
            let mut blocks = Vec::with_capacity(n);
            for _ in 0..n {
                // use small len here to increase the chance of duplication
                let data = [u8::arbitrary(g), u8::arbitrary(g)];
                let cid = Cid::new_v0(multihash::Code::Sha2_256.digest(&data)).unwrap();
                let block = Block {
                    cid,
                    data: data.to_vec(),
                };
                blocks.push(block);
            }
            Self(blocks)
        }
    }

    #[quickcheck]
    fn car_dedup_reader_tests(a: Blocks, b: Blocks) -> anyhow::Result<()> {
        let unique_len = {
            let mut set = HashSet::from_iter(a.0.iter().map(|b| b.cid).collect::<Vec<Cid>>());
            for block in &b.0 {
                set.insert(block.cid);
            }
            set.len()
        };

        println!(
            "a.len: {}, b.len:{}, total: {}, unique: {unique_len}",
            a.0.len(),
            b.0.len(),
            a.0.len() + b.0.len(),
        );

        let rt = tokio::runtime::Runtime::new()?;
        let count = rt.block_on(async move {
            let mut reader = MultiCarDedupReader::new(vec![a.0.into_iter(), b.0.into_iter()]);
            let mut count = 0usize;
            while let Ok(Some(_)) = reader.next_block().await {
                count += 1;
            }
            count
        });

        assert_eq!(count, unique_len);

        Ok(())
    }
}

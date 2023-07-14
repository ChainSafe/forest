// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use cid::Cid;
use clap::Subcommand;
use futures::{AsyncRead, Stream, StreamExt, TryStreamExt};
use fvm_ipld_car::CarHeader;
use fvm_ipld_car::CarReader;
use tokio_util::compat::TokioAsyncReadCompatExt;

use crate::ipld::CidHashSet;

type BlockPair = (Cid, Vec<u8>);

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
                    .then(tokio::fs::File::open)
                    .map_ok(tokio::io::BufReader::new)
                    .map_ok(tokio::io::BufReader::compat)
                    .map_err(fvm_ipld_car::Error::from)
                    .and_then(CarReader::new)
                    .try_collect()
                    .await?;

                let roots = {
                    let mut roots = vec![];
                    let mut seen = CidHashSet::default();
                    for reader in &readers {
                        for &root in &reader.header.roots {
                            if seen.insert(root) {
                                roots.push(root);
                            }
                        }
                    }
                    roots
                };

                let car_writer = CarHeader::from(roots);
                let mut output_file =
                    tokio::io::BufWriter::new(tokio::fs::File::create(output).await?).compat();

                car_writer
                    .write_stream_async(
                        &mut output_file,
                        &mut Box::pin(dedup_block_stream(merge_car_readers(readers))),
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

fn read_car_as_stream<R>(reader: CarReader<R>) -> impl Stream<Item = BlockPair>
where
    R: AsyncRead + Send + Unpin,
{
    futures::stream::unfold(reader, move |mut reader| async {
        reader
            .next_block()
            .await
            .expect("Failed to call CarReader::next_block")
            .map(|b| ((b.cid, b.data), reader))
    })
}

fn merge_car_readers<R>(readers: Vec<CarReader<R>>) -> impl Stream<Item = BlockPair>
where
    R: AsyncRead + Send + Unpin,
{
    futures::stream::iter(readers).flat_map(read_car_as_stream)
}

fn dedup_block_stream(stream: impl Stream<Item = BlockPair>) -> impl Stream<Item = BlockPair> {
    let mut seen = CidHashSet::default();
    stream.filter(move |(cid, _data)| futures::future::ready(seen.insert(*cid)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ahash::HashSet;
    use cid::multihash;
    use cid::multihash::MultihashDigest;
    use cid::Cid;
    use fvm_ipld_car::Block;
    use fvm_ipld_encoding::DAG_CBOR;
    use pretty_assertions::assert_eq;
    use quickcheck::Arbitrary;
    use quickcheck_macros::quickcheck;

    #[derive(Debug, Clone)]
    struct Blocks(Vec<Block>);

    impl Blocks {
        async fn into_car_bytes(self) -> anyhow::Result<Vec<u8>> {
            // Dummy root
            let writer = CarHeader::from(vec![self.0[0].cid]);
            let mut car = vec![];
            let mut stream = Box::pin(futures::stream::iter(self.0).map(|b| (b.cid, b.data)));
            writer.write_stream_async(&mut car, &mut stream).await?;
            Ok(car)
        }
    }

    impl Arbitrary for Blocks {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let n = loop {
                let n = u16::arbitrary(g) as usize;
                if n > 0 {
                    break n;
                }
            };
            let mut blocks = Vec::with_capacity(n);
            for _ in 0..n {
                // use small len here to increase the chance of duplication
                let data = [u8::arbitrary(g), u8::arbitrary(g)];
                let cid = Cid::new_v1(DAG_CBOR, multihash::Code::Blake2b256.digest(&data));
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
    fn car_dedup_block_stream_tests(a: Blocks, b: Blocks) -> anyhow::Result<()> {
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
            let car_a = a.into_car_bytes().await?;
            let car_b = b.into_car_bytes().await?;
            let deduped = dedup_block_stream(merge_car_readers(vec![
                CarReader::new(car_a.as_slice()).await?,
                CarReader::new(car_b.as_slice()).await?,
            ]));
            let count = deduped.count().await;
            Ok::<_, anyhow::Error>(count)
        })?;

        assert_eq!(count, unique_len);

        Ok(())
    }
}

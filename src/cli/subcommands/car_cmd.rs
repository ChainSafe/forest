// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use clap::Subcommand;
use futures::{Stream, StreamExt, TryStreamExt};
use itertools::Itertools;
use tokio::io::{AsyncBufRead, AsyncSeek, AsyncWriteExt};

use crate::ipld::CidHashSet;
use crate::utils::db::car_stream::{Block, CarStream};

#[derive(Debug, Subcommand)]
pub enum CarCommands {
    Concat {
        /// A list of CAR file paths. A CAR file can be a plain CAR, a zstd compressed CAR
        /// or a `.forest.car.zst` file
        car_files: Vec<PathBuf>,
        /// The output `ForestCAR.zst`(`.forest.car.zst`) file path
        #[arg(short, long)]
        output: PathBuf,
    },
}

impl CarCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Concat { car_files, output } => {
                let car_streams: Vec<_> = futures::stream::iter(car_files)
                    .then(tokio::fs::File::open)
                    .map_ok(tokio::io::BufReader::new)
                    .and_then(CarStream::new)
                    .try_collect()
                    .await?;

                let all_roots = car_streams
                    .iter()
                    .flat_map(|it| it.header.roots.iter())
                    .unique()
                    .cloned()
                    .collect::<Vec<_>>();

                let frames = crate::db::car::forest::Encoder::compress_stream(
                    8000_usize.next_power_of_two(),
                    zstd::DEFAULT_COMPRESSION_LEVEL as _,
                    dedup_block_stream(merge_car_streams(car_streams)).map_err(anyhow::Error::from),
                );
                let mut writer = tokio::io::BufWriter::new(tokio::fs::File::create(&output).await?);
                crate::db::car::forest::Encoder::write(&mut writer, all_roots, frames).await?;
                writer.flush().await?;
            }
        }
        Ok(())
    }
}

fn merge_car_streams<R>(
    car_streams: Vec<CarStream<R>>,
) -> impl Stream<Item = std::io::Result<Block>>
where
    R: AsyncSeek + AsyncBufRead + Unpin,
{
    futures::stream::iter(car_streams).flatten()
}

fn dedup_block_stream(
    stream: impl Stream<Item = std::io::Result<Block>>,
) -> impl Stream<Item = std::io::Result<Block>> {
    let mut seen = CidHashSet::default();
    stream.try_filter(move |Block { cid, data: _ }| futures::future::ready(seen.insert(*cid)))
}

#[cfg(test)]
mod tests {
    use std::pin::pin;

    use super::*;
    use ahash::HashSet;
    use cid::multihash;
    use cid::multihash::MultihashDigest;
    use cid::Cid;
    use futures::executor::{block_on, block_on_stream};
    use fvm_ipld_car::CarHeader;
    use fvm_ipld_encoding::DAG_CBOR;
    use pretty_assertions::assert_eq;
    use quickcheck::Arbitrary;
    use quickcheck_macros::quickcheck;

    #[derive(Debug, Clone)]
    struct Blocks(Vec<Block>);

    impl From<&Blocks> for HashSet<Cid> {
        fn from(blocks: &Blocks) -> Self {
            blocks.0.iter().map(|b| b.cid).collect()
        }
    }

    impl Blocks {
        async fn into_car_bytes(self) -> Vec<u8> {
            // Dummy root
            let writer = CarHeader::from(vec![self.0[0].cid]);
            let mut car = vec![];
            let mut stream = pin!(futures::stream::iter(self.0).map(|b| (b.cid, b.data)));
            writer
                .write_stream_async(&mut car, &mut stream)
                .await
                .unwrap();
            car
        }

        fn into_stream(self) -> impl Stream<Item = std::io::Result<Block>> {
            futures::stream::iter(self.0.into_iter().map(Ok::<_, std::io::Error>))
        }

        /// Implicit clone is performed inside to simplify caller code
        fn to_stream(&self) -> impl Stream<Item = std::io::Result<Block>> {
            self.clone().into_stream()
        }
    }

    impl Arbitrary for Blocks {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            // `CarReader` complains when n is 0: Error: Failed to parse CAR file: empty CAR file
            let n = u8::arbitrary(g).saturating_add(1) as usize;
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
    fn blocks_roundtrip(blocks: Blocks) -> anyhow::Result<()> {
        block_on(async move {
            let car = blocks.into_car_bytes().await;
            let mut reader = CarStream::new(std::io::Cursor::new(car.clone())).await?;
            let mut blocks2 = vec![];
            while let Some(b) = reader.try_next().await? {
                blocks2.push(b);
            }
            let blocks2 = Blocks(blocks2);
            let car2 = blocks2.into_car_bytes().await;

            assert_eq!(car, car2);

            Ok::<_, anyhow::Error>(())
        })
    }

    #[quickcheck]
    fn dedup_block_stream_tests_a_a(a: Blocks) {
        // ∀A. A∪A = A
        assert_eq!(dedup_block_stream_wrapper(&a, &a), HashSet::from(&a));
    }

    #[quickcheck]
    fn dedup_block_stream_tests_a_b(a: Blocks, b: Blocks) {
        let union_a_b = dedup_block_stream_wrapper(&a, &b);
        let union_b_a = dedup_block_stream_wrapper(&b, &a);
        // ∀AB. A∪B = B∪A
        assert_eq!(union_a_b, union_b_a);
        // ∀AB. A⊆(A∪B)
        union_a_b.is_superset(&HashSet::from(&a));
        // ∀AB. B⊆(A∪B)
        union_a_b.is_superset(&HashSet::from(&b));
    }

    fn dedup_block_stream_wrapper(a: &Blocks, b: &Blocks) -> HashSet<Cid> {
        let blocks: Vec<Cid> =
            block_on_stream(dedup_block_stream(a.to_stream().chain(b.to_stream())))
                .map(|block| block.unwrap().cid)
                .collect();

        // Ensure `dedup_block_stream` works properly
        assert!(blocks.iter().all_unique());

        HashSet::from_iter(blocks)
    }

    #[quickcheck]
    fn car_dedup_block_stream_tests(a: Blocks, b: Blocks) -> anyhow::Result<()> {
        let cid_union = HashSet::from_iter(HashSet::from(&a).union(&HashSet::from(&b)).cloned());

        block_on(async move {
            let car_a = std::io::Cursor::new(a.into_car_bytes().await);
            let car_b = std::io::Cursor::new(b.into_car_bytes().await);
            let mut deduped = pin!(dedup_block_stream(merge_car_streams(vec![
                CarStream::new(car_a).await?,
                CarStream::new(car_b).await?,
            ])));

            let mut cid_union2 = HashSet::default();
            while let Some(Block { cid, data: _ }) = deduped.try_next().await? {
                cid_union2.insert(cid);
            }

            assert_eq!(cid_union, cid_union2);

            Ok::<_, anyhow::Error>(())
        })
    }
}

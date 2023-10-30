//! Despite colliding very frequently on e.g [`quickcheck::Arbitrary`] [`Cid`]s,
//! and failing [typical hash quality tests](https://hbfs.wordpress.com/2015/10/13/testing-hash-functions-hash-functions-part-iii/),
//! our [custom] hash function for `.forest.car.zst` embedded indexes yields very
//! few collisions _in practice_, which can be demonstrated by running this
//! script on a few snapshot files.

use std::{
    hash::{Hash, Hasher},
    io,
    path::PathBuf,
};

use ahash::{AHashMap as HashMap, AHasher};
use async_compression::tokio::bufread::ZstdDecoder;
use bytes::{Buf as _, Bytes};
use cid::Cid;
use clap::Parser;
use futures::{stream, Stream, StreamExt as _, TryStreamExt as _};
use hashers::{
    builtin::DefaultHasher,
    fnv::{FNV1aHasher32, FNV1aHasher64},
    fx_hash::{FxHasher32, FxHasher64},
    jenkins::{spooky_hash::SpookyHasher, Lookup3Hasher, OAATHasher},
    null::{NullHasher, PassThroughHasher},
    oz::{DJB2Hasher, LoseLoseHasher, SDBMHasher},
    pigeon::Bricolage,
};
use itertools::Itertools as _;
use quickcheck::Arbitrary;
use siphasher::sip::{SipHasher13, SipHasher24};
use tokio::{fs::File, io::BufReader};
use tokio_util::codec::FramedRead;
use tokio_util::either::Either;

/// Measure collisions for various hash functions when hashing CIDs.
#[derive(Parser)]
enum Args2 {
    /// Read (presumably real-world) CIDs from a file.
    Files {
        /// A list of zstd-compressed CAR files.
        #[arg(required = true, num_args(1..))]
        files: Vec<PathBuf>,
        /// Sample the first N CIDs read from the files.
        ///
        /// This is non-deterministic for more than one file.
        #[arg(long)]
        take: Option<usize>,
        /// Print the table after every n sampled CIDs.
        #[arg(long)]
        progress: Option<u32>,
    },
    /// Generate arbitrary CIDs.
    Arbitrary {
        #[arg(long)]
        take: Option<usize>,
        #[arg(long, required_unless_present = "take" /* prevent silent infinite loop */)]
        progress: Option<u32>,
    },
}
#[tokio::main]
async fn main() -> io::Result<()> {
    _main2(Args2::parse()).await
}

async fn _main2(args: Args2) -> io::Result<()> {
    let (cids, progress) = open(args);
    let (count, table) = cids
        .try_fold(
            (0u32, HashMap::<&str, HashMap<u64, u32>>::new()),
            |(count, mut table), cid| async move {
                for (name, hash) in [
                    ("ours", u64::from(custom::Hash::from(cid))),
                    ("ahash", hash_once::<AHasher>(cid)),
                    ("sip13", hash_once::<SipHasher13>(cid)),
                    ("sip24", hash_once::<SipHasher24>(cid)),
                    ("builtin", hash_once::<DefaultHasher>(cid)),
                    ("fnv32", hash_once::<FNV1aHasher32>(cid)),
                    ("fvn64", hash_once::<FNV1aHasher64>(cid)),
                    ("fx32", hash_once::<FxHasher32>(cid)),
                    ("fx64", hash_once::<FxHasher64>(cid)),
                    ("spooky", hash_once::<SpookyHasher>(cid)),
                    ("lookup3", hash_once::<Lookup3Hasher>(cid)),
                    ("oaat", hash_once::<OAATHasher>(cid)),
                    ("null", hash_once::<NullHasher>(cid)),
                    ("passthrough", hash_once::<PassThroughHasher>(cid)),
                    ("djb2", hash_once::<DJB2Hasher>(cid)),
                    ("loselose", hash_once::<LoseLoseHasher>(cid)),
                    ("sdbm", hash_once::<SDBMHasher>(cid)),
                    ("bricolage", hash_once::<Bricolage>(cid)),
                ] {
                    table
                        .entry(name)
                        .or_default()
                        .entry(hash)
                        .and_modify(|it| *it += 1)
                        .or_insert(1);
                }
                let count = count + 1;
                if let Some(progress) = progress {
                    if count % progress == 0 {
                        print_table(count, &table)
                    }
                }
                Ok((count, table))
            },
        )
        .await?;

    print_table(count, &table);
    Ok(())
}

fn open(args: Args2) -> (impl Stream<Item = io::Result<Cid>>, Option<u32>) {
    match args {
        Args2::Files {
            files,
            take,
            progress,
        } => {
            let cids = stream::iter(files)
                .then(|path| async move {
                    let mut decoder = ZstdDecoder::new(BufReader::new(File::open(path).await?));
                    decoder.multiple_members(true); // thanks for the footgun...
                    let frames = FramedRead::new(
                        decoder,
                        unsigned_varint::codec::UviBytes::<Bytes>::default(),
                    )
                    .skip(1); // skip the header
                    io::Result::Ok(frames)
                })
                .try_flatten()
                .and_then(|frame| async {
                    match Cid::read_bytes(frame.reader()) {
                        Ok(cid) => Ok(cid),
                        Err(cid::Error::Io(e)) => Err(e),
                        Err(error) => Err(io::Error::new(io::ErrorKind::InvalidData, error)),
                    }
                });
            match take {
                Some(take) => (Either::Left(Either::Left(cids.take(take))), progress),
                None => (Either::Left(Either::Right(cids)), progress),
            }
        }
        Args2::Arbitrary { take, progress } => {
            let mut gen = quickcheck::Gen::new(1024);
            let cids = stream::repeat_with(move || Ok(Cid::arbitrary(&mut gen)));
            match take {
                Some(take) => (Either::Right(Either::Left(cids.take(take))), progress),
                None => (Either::Right(Either::Right(cids)), progress),
            }
        }
    }
}

fn hash_once<H: Hasher + Default>(t: impl Hash) -> u64 {
    let mut hasher = H::default();
    t.hash(&mut hasher);
    hasher.finish()
}

fn print_table(n: u32, table: &HashMap<&str, HashMap<u64, u32>>) {
    println!("cids: {}", n);
    for (name, ncollisions) in table
        .iter()
        .map(|(name, seen)| (name, seen.values().filter(|it| **it > 1).sum::<u32>()))
        .sorted_by_key(|(_, n)| *n)
    {
        println!("\t{:>12}: {:>10} collisions", name, ncollisions)
    }
}

mod custom {
    use cid::Cid;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Hash(u64);

    impl From<Hash> for u64 {
        fn from(Hash(hash): Hash) -> u64 {
            hash
        }
    }

    impl From<u64> for Hash {
        fn from(hash: u64) -> Hash {
            Hash(hash.saturating_sub(1))
        }
    }

    impl From<Cid> for Hash {
        fn from(cid: Cid) -> Hash {
            cid.hash()
                .digest()
                .chunks_exact(8)
                .map(<[u8; 8]>::try_from)
                .filter_map(Result::ok)
                .fold(cid.codec() ^ cid.hash().code(), |hash, chunk| {
                    hash ^ u64::from_le_bytes(chunk)
                })
                .into()
        }
    }
}

//! This module contains modified source code of https://crates.io/crates/rs-car
//!
//! Rust implementation of the [CAR specifications](https://ipld.io/specs/transport/car/),
//! both [CARv1](https://ipld.io/specs/transport/car/carv1/) and [CARv2](https://ipld.io/specs/transport/car/carv2/).
//!
//! # Usage
//!
//! - To get a block streamer [`CarReader::new()`]
//! - To read all blocks in memory [`car_read_all`]
//!

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::{future::BoxFuture, AsyncRead, Stream, StreamExt};
pub use ipld_core::cid::Cid;

use self::{
    block_cid::assert_block_cid,
    car_block::decode_block,
    car_header::{read_car_header, StreamEnd},
};
pub use self::{car_header::CarHeader, error::CarDecodeError};

mod block_cid;
mod car_block;
mod car_header;
mod carv1_header;
mod carv2_header;
mod error;
mod varint;

/// Decodes a CAR stream yielding its blocks and optionally verifying integrity.
/// Supports CARv1 and CARv2 formats.
///
/// - To get a block streamer [`CarReader::new()`]
/// - To read all blocks in memory [`car_read_all`]
pub struct CarReader<'a, R> {
    // r: &'a mut R,
    pub header: CarHeader,
    read_bytes: usize,
    validate_block_hash: bool,
    decode_header_future: Option<DecodeBlockFuture<'a, R>>,
}

impl<'a, R> CarReader<'a, R>
where
    R: AsyncRead + Send + Unpin,
{
    /// Decodes a CAR stream up to the header. Returns a `Stream` type that yields
    /// blocks. The CAR header is available in [`CarReader.header`].
    ///
    /// # Examples
    /// ```ignore
    /// use rs_car::{CarReader, CarDecodeError};
    /// use futures::StreamExt;
    ///
    /// #[async_std::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///   let mut r = async_std::fs::File::open("./tests/custom_fixtures/helloworld.car").await?;
    ///
    ///   let mut car_reader = CarReader::new(&mut r, true).await?;
    ///   println!("{:?}", car_reader.header);
    ///
    ///   while let Some(item) = car_reader.next().await {
    ///     let (cid, block) = item?;
    ///     println!("{:?} {} bytes", cid, block.len());
    ///   }
    ///
    ///   Ok(())
    /// }
    /// ```
    pub async fn new(
        r: &'a mut R,
        validate_block_hash: bool,
    ) -> Result<CarReader<'a, R>, CarDecodeError> {
        let header = read_car_header(r).await?;
        return Ok(CarReader {
            header,
            read_bytes: 0,
            validate_block_hash,
            decode_header_future: Some(Box::pin(decode_block(r))),
        });
    }
}

/// Decodes a CAR stream buffering all blocks in memory. For a Stream API use [`CarReader`].
///
/// # Examples
///
/// ```ignore
/// use rs_car::car_read_all;
///
/// #[async_std::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///   let mut r = async_std::fs::File::open("./tests/custom_fixtures/helloworld.car").await?;
///
///   let (blocks, header) = car_read_all(&mut r, true).await?;
///   println!("{:?}", header);
///
///   for (cid, block) in blocks {
///     println!("{:?} {} bytes", cid, block.len());
///   }
///
///   Ok(())
/// }
/// ```
pub async fn car_read_all<R: AsyncRead + Unpin + Send>(
    r: &mut R,
    validate_block_hash: bool,
) -> Result<(Vec<(Cid, Vec<u8>)>, CarHeader), CarDecodeError> {
    let mut decoder = CarReader::new(r, validate_block_hash).await?;
    let mut items: Vec<(Cid, Vec<u8>)> = vec![];

    while let Some(item) = decoder.next().await {
        let item = item?;
        items.push(item);
    }

    Ok((items, decoder.header))
}

type DecodeBlockFuture<'a, R> =
    BoxFuture<'a, Result<(&'a mut R, Cid, Vec<u8>, usize), CarDecodeError>>;

impl<'a, R> Stream for CarReader<'a, R>
where
    R: AsyncRead + Send + Unpin + 'a,
{
    type Item = Result<(Cid, Vec<u8>), CarDecodeError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let me = Pin::into_inner(self);

        if let StreamEnd::AfterNBytes(blocks_len) = me.header.eof_stream {
            if me.read_bytes >= blocks_len {
                return Poll::Ready(None);
            }
        }

        match &mut me.decode_header_future {
            Some(decode_future) => match decode_future.as_mut().poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Ok((r, cid, block, block_len))) => {
                    if me.validate_block_hash {
                        assert_block_cid(&cid, &block)?;
                    }
                    me.read_bytes += block_len;
                    me.decode_header_future = Some(Box::pin(decode_block(r)));
                    Poll::Ready(Some(Ok((cid, block))))
                }
                Poll::Ready(Err(CarDecodeError::BlockStartEOF))
                    if me.header.eof_stream == StreamEnd::OnBlockEOF =>
                {
                    Poll::Ready(None)
                }
                Poll::Ready(Err(err)) => {
                    me.decode_header_future = None;
                    Poll::Ready(Some(Err(err)))
                }
            },
            None => Poll::Ready(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, str::FromStr};

    use futures::executor;
    use serde::{Deserialize, Serialize};

    use self::car_header::CarVersion;
    use super::*;

    #[derive(Debug, Deserialize, Serialize)]
    struct ExpectedCarv1 {
        header: ExpectedCarv1Header,
        blocks: Vec<ExpectedCarBlock>,
    }

    #[derive(Debug, Deserialize, Serialize)]
    struct ExpectedCarv1Header {
        roots: Vec<ExpectedCid>,
        version: u8,
    }

    #[derive(Debug, Deserialize, Serialize)]
    #[allow(non_snake_case)]
    struct ExpectedCarBlock {
        cid: ExpectedCid,
        blockLength: usize,
    }

    type ExpectedCid = HashMap<String, String>;

    fn parse_expected_cids(cids: &[ExpectedCid]) -> Vec<Cid> {
        cids.iter().map(parse_expected_cid).collect()
    }

    fn parse_expected_cid(cid: &ExpectedCid) -> Cid {
        Cid::from_str(cid.get("/").unwrap()).unwrap()
    }

    #[test]
    #[ignore]
    fn decode_carv1_helloworld_no_stream() {
        executor::block_on(async {
            let car_filepath = "./tests/custom_fixtures/helloworld.car";
            let mut file = async_std::fs::File::open(car_filepath).await.unwrap();
            let (blocks, header) = car_read_all(&mut file, true).await.unwrap();

            let root_cid = Cid::from_str("QmUU2HcUBVSXkfWPUc3WUSeCMrWWeEJTuAgR9uyWBhh9Nf").unwrap();
            let root_block = hex::decode("0a110802120b68656c6c6f776f726c640a180b").unwrap();

            assert_eq!(blocks, vec!((root_cid, root_block)));
            assert_eq!(header.version, CarVersion::V1);
            assert_eq!(header.roots, vec!(root_cid));
        })
    }

    #[test]
    #[ignore]
    fn decode_carv1_helloworld_stream() {
        executor::block_on(async {
            let car_filepath = "./tests/custom_fixtures/helloworld.car";
            let mut file = async_std::fs::File::open(car_filepath).await.unwrap();
            let (blocks, header) = car_read_all(&mut file, true).await.unwrap();

            let root_cid = Cid::from_str("QmUU2HcUBVSXkfWPUc3WUSeCMrWWeEJTuAgR9uyWBhh9Nf").unwrap();
            let root_block = hex::decode("0a110802120b68656c6c6f776f726c640a180b").unwrap();

            assert_eq!(blocks, vec!((root_cid, root_block)));
            assert_eq!(header.version, CarVersion::V1);
            assert_eq!(header.roots, vec!(root_cid));
        })
    }

    #[test]
    #[ignore]
    fn decode_carv1_basic() {
        // 63a265726f6f747382d82a582500
        // 01711220f88bc853804cf294fe417e4fa83028689fcdb1b1592c5102e1474dbc200fab8b - v1 header root (bafyreihyrpefhacm6kkp4ql6j6udakdit7g3dmkzfriqfykhjw6cad5lrm)
        // d82a582500
        // 0171122069ea0740f9807a28f4d932c62e7c1c83be055e55072c90266ab3e79df63a365b - v1 header root (bafyreidj5idub6mapiupjwjsyyxhyhedxycv4vihfsicm2vt46o7morwlm)
        // 6776657273696f6e01
        // 5b - block 0 len = 91, block_len = 55
        // 01711220f88bc853804cf294fe417e4fa83028689fcdb1b1592c5102e1474dbc200fab8b - block 0 cid (bafyreihyrpefhacm6kkp4ql6j6udakdit7g3dmkzfriqfykhjw6cad5lrm)
        // a2646c696e6bd82a582300122002acecc5de2438ea4126a3010ecb1f8a599c8eff22fff1a1dcffe999b27fd3de646e616d6564626c6970 - block 0 data
        // 8301 - block 1 len = 131, block_len = 97
        // 122002acecc5de2438ea4126a3010ecb1f8a599c8eff22fff1a1dcffe999b27fd3de - block 1 cid (QmNX6Tffavsya4xgBi2VJQnSuqy9GsxongxZZ9uZBqp16d)
        // 122e0a2401551220b6fbd675f98e2abd22d4ed29fdc83150fedc48597e92dd1a7a24381d44a274511204626561721804122f0a22122079a982de3c9907953d4d323cee1d0fb1ed8f45f8ef02870c0cb9e09246bd530a12067365636f6e64189501 - block 1 data
        // 28 - block 2 len = 40, block_len = 4
        // 01551220b6fbd675f98e2abd22d4ed29fdc83150fedc48597e92dd1a7a24381d44a27451 - block 2 cid (bafkreifw7plhl6mofk6sfvhnfh64qmkq73oeqwl6sloru6rehaoujituke)
        // 63636363 - block 2 data
        // 8001 - block 3 len = 128, block_len = 94
        // 122079a982de3c9907953d4d323cee1d0fb1ed8f45f8ef02870c0cb9e09246bd530a - block 3 cid (QmWXZxVQ9yZfhQxLD35eDR8LiMRsYtHxYqTFCBbJoiJVys)
        // 122d0a240155122081cc5b17018674b401b42f35ba07bb79e211239c23bffe658da1577e3e6468771203646f671804122d0a221220e7dc486e97e6ebe5cdabab3e392bdad128b6e09acc94bb4e2aa2af7b986d24d0120566697273741833 - block 3 data
        // 28 - block 4 len = 40, block_len = 4
        // 0155122081cc5b17018674b401b42f35ba07bb79e211239c23bffe658da1577e3e646877 - block 4 cid(bafkreiebzrnroamgos2adnbpgw5apo3z4iishhbdx77gldnbk57d4zdio4)
        // 62626262 - block 4 data
        // 51 - block 5 len = 81, block_len = 47
        // 1220e7dc486e97e6ebe5cdabab3e392bdad128b6e09acc94bb4e2aa2af7b986d24d0 - block 5 cid (QmdwjhxpxzcMsR3qUuj7vUL8pbA7MgR3GAxWi2GLHjsKCT)
        // 122d0a240155122061be55a8e2f6b4e172338bddf184d6dbee29c98853e0a0485ecee7f27b9af0b412036361741804 - block 5 data
        // 28 - block 6 len = 40, block_len = 4
        // 0155122061be55a8e2f6b4e172338bddf184d6dbee29c98853e0a0485ecee7f27b9af0b4 - block 6 cid (bafkreidbxzk2ryxwwtqxem4l3xyyjvw35yu4tcct4cqeqxwo47zhxgxqwq)
        // 61616161 - block 6 data
        // 36 - block 7 len = 54, block_len = 18
        // 0171122069ea0740f9807a28f4d932c62e7c1c83be055e55072c90266ab3e79df63a365b - block 7 cid (bafyreidj5idub6mapiupjwjsyyxhyhedxycv4vihfsicm2vt46o7morwlm)
        // a2646c696e6bf6646e616d65656c696d626f - block 7 data
        executor::block_on(async {
            run_car_basic_test(
                "./tests/spec_fixtures/carv1-basic.car",
                "./tests/spec_fixtures/carv1-basic.json",
            )
            .await;
        })
    }

    #[test]
    #[ignore]
    fn decode_carv2_basic() {
        // 0aa16776657273696f6e02  - v2 pragma
        // 00000000000000000000000000000000  - v2 header characteristics
        // 3300000000000000  - v2 header data_offset
        // c001000000000000  - v2 header data_size
        // f301000000000000  - v2 header index_offset
        // 38a265726f6f747381d82a582300
        // 1220fb16f5083412ef1371d031ed4aa239903d84efdadf1ba3cd678e6475b1a232f8 - v1 header root (QmfEoLyB5NndqeKieExd1rtJzTduQUPEV8TwAYcUiy3H5Z)
        // 6776657273696f6e01
        // 51 - block 0 len = 81, block_len = 47
        // 1220fb16f5083412ef1371d031ed4aa239903d84efdadf1ba3cd678e6475b1a232f8 - block 0 cid (QmfEoLyB5NndqeKieExd1rtJzTduQUPEV8TwAYcUiy3H5Z)
        // 122d0a221220d9c0d5376d26f1931f7ad52d7acc00fc1090d2edb0808bf61eeb0a152826f6261204f09f8da418a401 - block 0 data
        // 8501 -  block 1 len = 133, block_len = 99
        // 1220d9c0d5376d26f1931f7ad52d7acc00fc1090d2edb0808bf61eeb0a152826f626 - block 1 cid (QmczfirA7VEH7YVvKPTPoU69XM3qY4DC39nnTsWd4K3SkM)
        // 12310a221220d745b7757f5b4593eeab7820306c7bc64eb496a7410a0d07df7a34ffec4b97f1120962617272656c657965183a122e0a2401551220a2e1c40da1ae335d4dffe729eb4d5ca23b74b9e51fc535f4a804a261080c294d1204f09f90a11807 - block 1 data
        // 58 - block 2 len = 88, block_len = 54
        // 1220d745b7757f5b4593eeab7820306c7bc64eb496a7410a0d07df7a34ffec4b97f1 - block 2 cid (Qmcpz2FHJD7VAhg1fxFXdYJKePtkx1BsHuCrAgWVnaHMTE)
        // 12340a2401551220b474a99a2705e23cf905a484ec6d14ef58b56bbe62e9292783466ec363b5072d120a666973686d6f6e6765721804 - block 2 data
        // 28 - block 3 len = 40, block_len 4
        // 01551220b474a99a2705e23cf905a484ec6d14ef58b56bbe62e9292783466ec363b5072d - block 3 cid (bafkreifuosuzujyf4i6psbneqtwg2fhplc2wxptc5euspa2gn3bwhnihfu)
        // 66697368 - block 3 data
        // 2b - block 4 len = 43, block_len 7
        // 01551220a2e1c40da1ae335d4dffe729eb4d5ca23b74b9e51fc535f4a804a261080c294d - block 4 cid (bafkreifc4hca3inognou377hfhvu2xfchn2ltzi7yu27jkaeujqqqdbjju)
        // 6c6f6273746572 - block 4 data
        // 0100000028000000c800000000000000a2e1c40da1ae335d4dffe729eb4d5ca23b74b9e51fc535f4a804a261080c294d9401000000000000b474a99a2705e23cf905a484ec6d14ef58b56bbe62e9292783466ec363b5072d6b01000000000000d745b7757f5b4593eeab7820306c7bc64eb496a7410a0d07df7a34ffec4b97f11201000000000000d9c0d5376d26f1931f7ad52d7acc00fc1090d2edb0808bf61eeb0a152826f6268b00000000000000fb16f5083412ef1371d031ed4aa239903d84efdadf1ba3cd678e6475b1a232f83900000000000000

        executor::block_on(async {
            run_car_basic_test(
                "./tests/spec_fixtures/carv2-basic.car",
                "./tests/spec_fixtures/carv2-basic.json",
            )
            .await;
        })
    }

    async fn run_car_basic_test(car_filepath: &str, car_json_expected: &str) {
        let expected_car = std::fs::read_to_string(car_json_expected).unwrap();
        let expected_car: ExpectedCarv1 = serde_json::from_str(&expected_car).unwrap();

        let mut file = async_std::fs::File::open(car_filepath).await.unwrap();
        let mut streamer = CarReader::new(&mut file, true).await.unwrap();

        // Assert header v1
        assert_eq!(streamer.header.version as u8, expected_car.header.version);
        assert_eq!(
            streamer.header.roots,
            parse_expected_cids(&expected_car.header.roots)
        );

        // Consume stream and read all blocks into memory
        let mut blocks: Vec<(Cid, Vec<u8>)> = vec![];
        while let Some(item) = streamer.next().await {
            let item = item.unwrap();
            blocks.push(item);
        }

        // Assert block's cids, with validate_block_hash == true guarantees block's integrity
        let block_cids = blocks.iter().map(|block| block.0).collect::<Vec<Cid>>();
        let expected_block_cids = expected_car
            .blocks
            .iter()
            .map(|block| parse_expected_cid(&block.cid))
            .collect::<Vec<Cid>>();
        assert_eq!(block_cids, expected_block_cids);
    }
}

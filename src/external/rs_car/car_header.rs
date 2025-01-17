use futures::{AsyncRead, AsyncReadExt};

use super::{
    carv1_header::{decode_carv1_header, CarV1Header},
    carv2_header::{decode_carv2_header, CarV2Header, CARV2_HEADER_SIZE, CARV2_PRAGMA_SIZE},
    error::CarDecodeError,
    varint::read_varint_u64,
    Cid,
};

/// Arbitrary high value to prevent big allocations
const MAX_HEADER_LEN: u64 = 1048576;
/// Arbitrary high value to prevent big allocations
const MAX_PADDING_LEN: usize = 1073741824;

#[derive(Debug, PartialEq)]
pub(crate) enum StreamEnd {
    AfterNBytes(usize),
    OnBlockEOF,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum CarVersion {
    V1 = 1,
    V2 = 2,
}

#[derive(Debug)]
pub struct CarHeader {
    pub version: CarVersion,
    pub roots: Vec<Cid>,
    pub characteristics_v2: Option<u128>,
    pub(crate) eof_stream: StreamEnd,
}

impl CarHeader {}

pub(crate) async fn read_car_header<R: AsyncRead + Unpin>(
    r: &mut R,
) -> Result<CarHeader, CarDecodeError> {
    let (header, _) = read_carv1_header(r).await?;

    match header.version {
        1 => Ok(CarHeader {
            version: CarVersion::V1,
            roots: header.roots.ok_or(CarDecodeError::InvalidCarV1Header(
                "v1 header has not roots".to_owned(),
            ))?,
            characteristics_v2: None,
            eof_stream: StreamEnd::OnBlockEOF,
        }),
        2 => {
            let (header_v2, (header_v1, header_v1_len)) = read_carv2_header(r).await?;
            let blocks_len = header_v2.data_size as usize - header_v1_len;
            Ok(CarHeader {
                version: CarVersion::V2,
                roots: header_v1.roots.ok_or(CarDecodeError::InvalidCarV1Header(
                    "v1 header has not roots".to_owned(),
                ))?,
                characteristics_v2: Some(header_v2.characteristics),
                eof_stream: StreamEnd::AfterNBytes(blocks_len),
            })
        }
        _ => Err(CarDecodeError::UnsupportedCarVersion {
            version: header.version,
        }),
    }
}

/// # Returns
///
/// (header, total header byte length including varint)
async fn read_carv1_header<R: AsyncRead + Unpin>(
    src: &mut R,
) -> Result<(CarV1Header, usize), CarDecodeError> {
    // Decode header varint
    let (header_len, varint_len) =
        read_varint_u64(src)
            .await?
            .ok_or(CarDecodeError::InvalidCarV1Header(
                "invalid header varint".to_string(),
            ))?;

    if header_len > MAX_HEADER_LEN {
        return Err(CarDecodeError::InvalidCarV1Header(format!(
            "header len too big {}",
            header_len
        )));
    }

    let mut header_buf = vec![0u8; header_len as usize];
    src.read_exact(&mut header_buf).await?;

    let header = decode_carv1_header(&header_buf)?;

    Ok((header, header_len as usize + varint_len))
}

async fn read_carv2_header<R: AsyncRead + Unpin>(
    r: &mut R,
) -> Result<(CarV2Header, (CarV1Header, usize)), CarDecodeError> {
    let mut header_buf = [0u8; CARV2_HEADER_SIZE];
    r.read_exact(&mut header_buf).await?;

    let header_v2 = decode_carv2_header(&header_buf)?;

    // Read padding, and throw away
    let padding_len = header_v2.data_offset as usize - CARV2_PRAGMA_SIZE - CARV2_HEADER_SIZE;
    if padding_len > 0 {
        if padding_len > MAX_PADDING_LEN {
            return Err(CarDecodeError::InvalidCarV1Header(format!(
                "padding len too big {}",
                padding_len
            )));
        }
        let mut padding_buf = vec![0u8; padding_len];
        r.read_exact(&mut padding_buf).await?;
    }

    // Read inner CARv1 header
    let header_v1 = read_carv1_header(r).await?;

    Ok((header_v2, header_v1))
}

#[cfg(test)]
mod tests {
    use futures::{executor, io::Cursor};

    use super::super::{
        carv1_header::CarV1Header,
        carv2_header::{CARV2_PRAGMA, CARV2_PRAGMA_SIZE},
    };
    use super::*;

    #[test]
    fn read_carv1_header_v2_pragma() {
        executor::block_on(async {
            assert_eq!(
                read_carv1_header(&mut Cursor::new(&CARV2_PRAGMA))
                    .await
                    .unwrap(),
                (
                    CarV1Header {
                        version: 2,
                        roots: None
                    },
                    CARV2_PRAGMA_SIZE
                )
            )
        })
    }
}

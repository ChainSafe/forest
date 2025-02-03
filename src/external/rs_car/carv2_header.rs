use super::error::CarDecodeError;

pub(crate) const CARV2_HEADER_SIZE: usize = 40;
pub(crate) const CARV2_PRAGMA_SIZE: usize = 11;

// The pragma of a CARv2, containing the version number.
// This is a valid CARv1 header, with version number of 2 and no root CIDs.
#[allow(dead_code)]
pub(crate) const CARV2_PRAGMA: [u8; CARV2_PRAGMA_SIZE] = [
    0x0a, // unit(10)
    0xa1, // map(1)
    0x67, // string(7)
    0x76, 0x65, 0x72, 0x73, 0x69, 0x6f, 0x6e, // "version"
    0x02, // uint(2)
];

#[derive(Debug, PartialEq)]
pub(crate) struct CarV2Header {
    pub characteristics: u128,
    pub data_offset: u64,
    pub data_size: u64,
    pub index_offset: u64,
}

/// CARv2 header consists of:
/// - 11-byte pragma
/// - 40-byte header with characteristics and locations
/// - CARv1 data payload, including header, roots and sequence of CID:Bytes pairs
/// - Optional index for fast lookup
///
/// Full CARv2 stream
/// ```nn
/// [pragma][v2 header][opt padding][CARv1][opt padding][opt index]
/// ```
pub(crate) fn decode_carv2_header(
    header: &[u8; CARV2_HEADER_SIZE],
) -> Result<CarV2Header, CarDecodeError> {
    // 1. Characteristics: A 128-bit (16-byte) bitfield used to describe certain features of the enclosed data.
    // 2. Data offset: A 64-bit (8-byte) unsigned little-endian integer indicating the byte-offset from the beginning of the CARv2 to the first byte of the CARv1 data payload.
    // 3. Data size: A 64-bit (8-byte) unsigned little-endian integer indicating the byte-length of the CARv1 data payload.
    // 4. Index offset: A 64-bit (8-byte) unsigned little-endian integer indicating the byte-offset from the beginning of the CARv2 to the first byte of the index payload. This value may be 0 to indicate the absence of index data.
    let characteristics = u128::from_be_bytes(header[0..16].try_into().unwrap());
    let data_offset = u64::from_le_bytes(header[16..24].try_into().unwrap());
    let data_size = u64::from_le_bytes(header[24..32].try_into().unwrap());
    let index_offset = u64::from_le_bytes(header[32..40].try_into().unwrap());

    Ok(CarV2Header {
        characteristics,
        data_offset,
        data_size,
        index_offset,
    })
}

use blake2b_simd::Params;
use futures::{AsyncRead, AsyncReadExt};
use ipld_core::{cid, cid::multihash::Multihash};
use sha2::{Digest, Sha256};

use super::{
    error::{CarDecodeError, HashCode},
    varint::read_varint_u64,
    Cid,
};

const CODE_IDENTITY: u64 = 0x00;
const CODE_SHA2_256: u64 = 0x12;
const CODE_BLAKE2B_256: u64 = 0xb220;
const DIGEST_SIZE: usize = 64;
const CID_V0_MH_SIZE: usize = 32;

pub(crate) async fn read_block_cid<R: AsyncRead + Unpin>(
    src: &mut R,
) -> Result<(Cid, usize), CarDecodeError> {
    let (version, version_len) = read_varint_u64(src)
        .await?
        .ok_or(cid::Error::InvalidCidVersion)?;
    let (codec, codec_len) = read_varint_u64(src)
        .await?
        .ok_or(cid::Error::InvalidCidV0Codec)?;

    // A CIDv0 is indicated by a first byte of 0x12 followed by 0x20 which specifies a 32-byte (0x20) length SHA2-256 (0x12) digest.
    if [version, codec] == [CODE_SHA2_256, 0x20] {
        let mut digest = [0u8; CID_V0_MH_SIZE];
        src.read_exact(&mut digest).await?;
        let mh = Multihash::wrap(version, &digest).expect("Digest is always 32 bytes.");
        return Ok((Cid::new_v0(mh)?, version_len + codec_len + CID_V0_MH_SIZE));
    }

    // CIDv1 components:
    // 1. Version as an unsigned varint (should be 1)
    // 2. Codec as an unsigned varint (valid according to the multicodec table)
    // 3. The raw bytes of a multihash
    let version = cid::Version::try_from(version).unwrap();
    match version {
        cid::Version::V0 => Err(cid::Error::InvalidExplicitCidV0)?,
        cid::Version::V1 => {
            let (mh, mh_len) = read_multihash(src).await?;
            Ok((
                Cid::new(version, codec, mh)?,
                version_len + codec_len + mh_len,
            ))
        }
    }
}

async fn read_multihash<R: AsyncRead + Unpin>(
    r: &mut R,
) -> Result<(Multihash<DIGEST_SIZE>, usize), CarDecodeError> {
    let (code, code_len) = read_varint_u64(r)
        .await?
        .ok_or(CarDecodeError::InvalidMultihash(
            "invalid code varint".to_string(),
        ))?;
    let (size, size_len) = read_varint_u64(r)
        .await?
        .ok_or(CarDecodeError::InvalidMultihash(
            "invalid size varint".to_string(),
        ))?;

    if size > u8::MAX as u64 {
        panic!("digest size {} > max {}", size, DIGEST_SIZE)
    }

    let mut digest = [0; DIGEST_SIZE];
    r.read_exact(&mut digest[..size as usize]).await?;

    // Multihash does not expose a way to construct Self without some decoding or copying
    // unwrap: multihash must be valid since it's constructed manually
    let mh = Multihash::wrap(code, &digest[..size as usize]).unwrap();

    Ok((mh, code_len + size_len + size as usize))
}

pub(crate) fn assert_block_cid(cid: &Cid, block: &[u8]) -> Result<(), CarDecodeError> {
    let (hash_fn_name, block_digest) = match cid.hash().code() {
        CODE_IDENTITY => ("identity", block.to_vec()),
        CODE_SHA2_256 => ("sha2-256", hash_sha2_256(block).to_vec()),
        CODE_BLAKE2B_256 => ("blake2b-256", hash_blake2b_256(block).to_vec()),
        code => {
            return Err(CarDecodeError::UnsupportedHashCode((
                HashCode::Code(code),
                *cid,
            )));
        }
    };

    let cid_digest = cid.hash().digest();

    fn to_hex_lower(s: impl AsRef<[u8]>) -> String {
        s.as_ref()
            .iter()
            .map(|i| format!("{i:02x}"))
            .collect::<Vec<_>>()
            .as_slice()
            .join("")
    }

    if cid_digest != block_digest {
        return Err(CarDecodeError::BlockDigestMismatch(format!(
            "{} digest mismatch cid {:?} cid digest {} block digest {}",
            hash_fn_name,
            cid,
            to_hex_lower(cid_digest),
            to_hex_lower(block_digest),
        )));
    }

    Ok(())
}

fn hash_sha2_256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

fn hash_blake2b_256(data: &[u8]) -> [u8; 32] {
    Params::new()
        .hash_length(32)
        .to_state()
        .update(data)
        .finalize()
        .as_bytes()
        .try_into()
        .unwrap()
}

#[cfg(test)]
mod tests {
    use std::io;

    use futures::{executor, io::Cursor};
    use ipld_core::cid::{multihash::Multihash, Cid};

    use super::super::{block_cid::CODE_SHA2_256, error::CarDecodeError};
    use super::{assert_block_cid, read_block_cid, read_multihash};

    const CID_V0_STR: &str = "QmUU2HcUBVSXkfWPUc3WUSeCMrWWeEJTuAgR9uyWBhh9Nf";
    const CID_V0_HEX: &str = "12205b0995ced69229d26009c53c185a62ea805a339383521edbed1028c496615448";
    const CID_DIGEST: &str = "5b0995ced69229d26009c53c185a62ea805a339383521edbed1028c496615448";

    const CID_V1_STR: &str = "bafyreihyrpefhacm6kkp4ql6j6udakdit7g3dmkzfriqfykhjw6cad5lrm";
    const CID_V1_HEX: &str =
        "01711220f88bc853804cf294fe417e4fa83028689fcdb1b1592c5102e1474dbc200fab8b";

    // Cursor = easy way to get AsyncRead from an AsRef<[u8]>
    fn from_hex(input: &str) -> Cursor<Vec<u8>> {
        Cursor::new(hex::decode(input).unwrap())
    }

    #[test]
    fn read_block_cid_from_v0() {
        let cid_expected = Cid::try_from(CID_V0_STR).unwrap();

        let mut input_stream = from_hex(CID_V0_HEX);
        let (cid, cid_len) = executor::block_on(read_block_cid(&mut input_stream)).unwrap();

        assert_eq!(cid, cid_expected);
        assert_eq!(cid_len, cid_expected.to_bytes().len());
    }

    #[test]
    fn read_multihash_from_v0() {
        let digest = hex::decode(CID_DIGEST).unwrap();
        let mh_expected = Multihash::<64>::wrap(CODE_SHA2_256, &digest).unwrap();

        let mut input_stream = from_hex(CID_V0_HEX);
        let (mh, mh_len) = executor::block_on(read_multihash(&mut input_stream)).unwrap();

        assert_eq!(mh, mh_expected);
        assert_eq!(mh_len, mh_expected.to_bytes().len());

        // Sanity check, same result as sync version. Sync API can dynamically shrink size to 32 bytes
        let mh_sync = Multihash::<64>::read(&mut mh_expected.to_bytes().as_slice()).unwrap();
        assert_eq!(mh_sync, mh_expected);
    }

    #[test]
    fn read_block_cid_from_v1() {
        let cid_expected = Cid::try_from(CID_V1_STR).unwrap();

        let mut input_stream = from_hex(CID_V1_HEX);
        let (cid, cid_len) = executor::block_on(read_block_cid(&mut input_stream)).unwrap();

        // Double check multihash before full CID
        assert_eq!(cid.hash(), cid_expected.hash());

        assert_eq!(cid, cid_expected);
        assert_eq!(cid_len, cid_expected.to_bytes().len());
    }

    #[test]
    fn read_multihash_error_varint_unexpected_eof() {
        let mut input_stream = from_hex("ffff");

        match executor::block_on(read_multihash(&mut input_stream)) {
            Err(CarDecodeError::IoError(err)) => {
                assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof)
            }
            x => panic!("other result {:?}", x),
        }
    }

    #[test]
    fn assert_block_cid_v0_helloworld() {
        // simple dag-pb of string "helloworld"
        let cid = Cid::try_from("QmUU2HcUBVSXkfWPUc3WUSeCMrWWeEJTuAgR9uyWBhh9Nf").unwrap();
        let block = hex::decode("0a110802120b68656c6c6f776f726c640a180b").unwrap();
        assert_block_cid(&cid, &block).unwrap();
    }
}

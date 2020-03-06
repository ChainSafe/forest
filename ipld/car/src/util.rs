use super::error::Error;
use cid::Cid;
use forest_encoding::from_slice;
use std::io::Read;

pub(crate) fn ld_read<R: Read>(mut buf_reader: &mut R) -> Result<(u64, Vec<u8>), Error> {
    let l =
        unsigned_varint::io::read_u64(&mut buf_reader).map_err(|e| Error::Other(e.to_string()))?;
    let mut buf = Vec::with_capacity(l as usize);
    buf_reader.take(l).read_to_end(&mut buf);
    Ok((l, buf))
}

pub(crate) fn read_node<R: Read>(mut buf_reader: &mut R) -> Result<(Cid, Vec<u8>), Error> {
    let (l, buf) = ld_read(buf_reader)?;
    let (c, n) = read_cid(&buf)?;
    Ok((c, buf[(n as usize)..].to_owned()))
}

pub(crate) fn read_cid(buf: &[u8]) -> Result<(Cid, u64), Error> {
    // TODO: Add some checks for cid v0
    let (version, buf) = unsigned_varint::decode::u64(buf).unwrap();
    let (codec, multihash_with_data) = unsigned_varint::decode::u64(buf).unwrap();
    // multihash part
    let (hashcode, buf) = unsigned_varint::decode::u64(multihash_with_data).unwrap();
    let hashcode_len_diff = multihash_with_data.len() - buf.len();
    let (len, _) = unsigned_varint::decode::u64(buf).unwrap();
    let hash = &buf[0..len as usize];

    let cid: Cid = Cid::new(
        cid::Codec::from(codec)?,
        cid::Version::from(version)?,
        cid::multihash::Multihash::from_bytes(
            multihash_with_data[0..=(len as usize + hashcode_len_diff)].to_vec(),
        )?,
    );
    let len = cid.to_bytes().len() as u64;
    Ok((cid, len))
}

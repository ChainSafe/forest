use super::error::Error;
use cid::Cid;
use forest_encoding::from_slice;
use std::io::Read;

pub(crate) fn ld_read<R: Read>(mut buf_reader: &mut R) -> Result<(u64, Vec<u8>), Error> {
    let l = unsigned_varint::io::read_u64(&mut buf_reader).map_err(|e| Error::Other(e.to_string()))?;
    let mut buf = Vec::with_capacity(l as usize);
    buf_reader.take(l).read_to_end(&mut buf);
    Ok((l, buf))
}

pub(crate) fn read_node<R: Read>(mut buf_reader: &mut R) -> Result<(Cid, Vec<u8>),Error> {
    let (l, buf) = ld_read(buf_reader)?;
    let (c, n) = read_cid(&buf);
    Ok((c, buf[(n as usize)..].to_owned()))
}

pub(crate) fn read_cid(buf: &[u8]) -> (Cid, u64) {
    // TODO: Add some checks

    let x = buf;

    let (version, buf) = unsigned_varint::decode::u64(buf).unwrap();
    let (codec, buf) = unsigned_varint::decode::u64(buf).unwrap();
    let (hashcode, buf) = unsigned_varint::decode::u64(buf).unwrap();
    let (len, buf) = unsigned_varint::decode::u64(buf).unwrap();
    let hash = &buf[0..len as usize];

    let cid: Cid = Cid::new(
        cid::Codec::from(codec).unwrap(),
        cid::Version::from(version).unwrap(),
        cid::multihash::encode(
            cid::multihash::Hash::from_code(hashcode as u16).unwrap(),
            &hash,
        ).unwrap());
//    );
//    let prefix = cid::Prefix{
//        version: cid::Version::from(version).unwrap(),
//        codec: cid::Codec::from(codec).unwrap(),
//        mh_type: cid::multihash::Hash::from_code(hashcode as u16).unwrap(),
//        mh_len: len as usize
//    };
//   let cid: Cid = Cid::new_from_prefix(&prefix, &hash).unwrap();
//
//    let cid: Cid = Cid::from_raw_cid(&x[0..=(len-1)as usize]).unwrap();

    let len = cid.to_bytes().len() as u64;
    (cid, len)
}

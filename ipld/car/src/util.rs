use cid::Cid;
use forest_encoding::from_slice;
use std::io::Read;

pub(crate) fn ld_read<R: std::io::Read>(mut buf_reader: &mut R) -> (u64, Vec<u8>) {
    let l = unsigned_varint::io::read_u64(&mut buf_reader).unwrap();
    let mut buf = Vec::with_capacity(l as usize);
    buf_reader.take(l).read_to_end(&mut buf);
    (l, buf)
}

pub(crate) fn read_node<R: std::io::Read>(mut buf_reader: &mut R) -> (Cid, Vec<u8>) {
    let (l, buf) = ld_read(buf_reader);
    let (c, n) = read_cid(&buf);
    (c, buf[(n as usize)..].to_owned())
}

pub(crate) fn read_cid(buf: &[u8]) -> (Cid, u64) {
    // TODO: Add checks 0x12 0x20
    //   let cid: Cid = from_slice(buf[2..=34].as_ref()).unwrap() ;

    //    let (version, buf) = unsigned_varint::decode::u64(buf).unwrap();
    //    if version != 1 {
    //        panic!("Version is not 1")
    //    }

    let (version, buf) = unsigned_varint::decode::u64(buf).unwrap();
    let (codec, buf) = unsigned_varint::decode::u64(buf).unwrap();
    let (hashcode, buf) = unsigned_varint::decode::u64(buf).unwrap();
    let (len, buf) = unsigned_varint::decode::u64(buf).unwrap();
    let hash = &buf[0..len as usize];

    //    let cid: Cid = Cid::from_raw_cid(&buf[0..=37]).unwrap();
    let cid: Cid = Cid::new(
        cid::Codec::from(codec).unwrap(),
        cid::Version::from(version).unwrap(),
        cid::multihash::encode(
            cid::multihash::Hash::from_code(hashcode as u16).unwrap(),
            &hash,
        )
        .unwrap(),
    );

    let len =cid.to_bytes().len() as u64 ;
    (cid, len)
}

//func ReadCid(buf []byte) (cid.Cid, int, error) {
//if bytes.Equal(buf[:2], cidv0Pref) {
//c, err := cid.Cast(buf[:34])
//return c, 34, err
//}
//
//br := bytes.NewReader(buf)
//
//// assume cidv1
//vers, err := binary.ReadUvarint(br)
//if err != nil {
//return cid.Cid{}, 0, err
//}
//
//// TODO: the go-cid package allows version 0 here as well
//if vers != 1 {
//return cid.Cid{}, 0, fmt.Errorf("invalid cid version number")
//}
//
//codec, err := binary.ReadUvarint(br)
//if err != nil {
//return cid.Cid{}, 0, err
//}
//
//mhr := mh.NewReader(br)
//h, err := mhr.ReadMultihash()
//if err != nil {
//return cid.Cid{}, 0, err
//}
//
//return cid.NewCidV1(codec, h), len(buf) - br.Len(), nil
//}

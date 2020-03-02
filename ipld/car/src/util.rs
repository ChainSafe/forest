use std::io::Read;

pub(crate) fn ld_read<R: std::io::Read>(mut buf_reader: &mut R) -> (u64, Vec<u8>) {
    let l = unsigned_varint::io::read_u64(&mut buf_reader).unwrap();
    let mut buf = Vec::with_capacity(l as usize);
    buf_reader.take( l).read_to_end(&mut buf);
    (l, buf)
}

use std::io::Write as _;

use cid::Cid;
use futures::executor::block_on;
use fvm_ipld_car::CarHeader;
use tempfile::NamedTempFile;

fn make_car(
    roots: impl IntoIterator<Item = Cid>,
    frames: impl IntoIterator<Item = (Cid, Vec<u8>)>,
) -> NamedTempFile {
    let header = CarHeader {
        roots: roots.into_iter().collect(),
        version: 1,
    };
    let mut buffer = vec![];
    block_on(header.write_stream_async(&mut buffer, &mut futures::stream::iter(frames))).unwrap();
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(&buffer).unwrap();
    file
}

use std::io::Write as _;

use assert_cmd::{assert::Assert, Command};
use cid::{multihash::Multihash, Cid};
use futures::executor::block_on;
use fvm_ipld_car::CarHeader;
use tempfile::NamedTempFile;

#[test]
fn identical() {
    let left = make_car([Cid::default()], [(Cid::default(), vec![])]);
    let right = make_car([Cid::default()], [(Cid::default(), vec![])]);
    diff(left, right).success();
}

#[test]
fn different_cid() {
    let left = make_car([Cid::default()], [(Cid::default(), vec![])]);
    let right = make_car([Cid::default()], [(other_cid(), vec![])]);
    diff(left, right).failure();
}

#[test]
fn different_body() {
    let left = make_car([Cid::default()], [(Cid::default(), vec![0])]);
    let right = make_car([Cid::default()], [(Cid::default(), vec![1])]);
    diff(left, right).failure();
}

#[test]
fn longer() {
    let left = make_car([Cid::default()], []);
    let right = make_car([Cid::default()], [(Cid::default(), vec![])]);
    diff(left, right).failure();
}

#[test]
fn same_then_different() {
    let left = make_car(
        [Cid::default()],
        [(Cid::default(), vec![]), (Cid::default(), vec![])],
    );
    let right = make_car(
        [Cid::default()],
        [(Cid::default(), vec![]), (other_cid(), vec![])],
    );
    diff(left, right).failure();
}

fn diff(left: NamedTempFile, right: NamedTempFile) -> Assert {
    Command::cargo_bin("forest-cli")
        .unwrap()
        .args(["car", "diff"])
        .arg(left.path())
        .arg(right.path())
        .assert()
}

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

/// A non-default CID
fn other_cid() -> Cid {
    let cid = Cid::new_v1(1, Multihash::wrap(1, &[]).unwrap());
    assert_ne!(cid, Cid::default());
    cid
}

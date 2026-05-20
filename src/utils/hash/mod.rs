// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use digest::{Digest, Output, Update};
use digest_io::IoWrapper;
use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

#[allow(dead_code)]
pub fn digest_stream<H, R>(reader: &mut R) -> std::io::Result<Output<H>>
where
    H: Digest + Update,
    R: Read,
{
    let mut hasher = IoWrapper(H::new());
    std::io::copy(reader, &mut hasher)?;
    Ok(hasher.0.finalize())
}

#[allow(dead_code)]
pub fn digest_file<H>(path: impl AsRef<Path>) -> std::io::Result<Output<H>>
where
    H: Digest + Update,
{
    let mut reader = BufReader::new(File::open(path)?);
    digest_stream::<H, _>(&mut reader)
}

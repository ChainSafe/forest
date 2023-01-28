// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use std::path::PathBuf;

pub trait IndexedStore
where
    Self: ReadWriteStore + Sized,
{
    fn open(root_path: PathBuf, index: usize) -> anyhow::Result<Self>;
}

// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
mod any;
pub mod forest;
mod many;
pub mod plain;

pub use any::AnyCar;
pub use forest::ForestCar;
pub use many::ManyCar;
pub use plain::PlainCar;

use crate::utils::db::car_index::FrameOffset;
use ahash::HashMap;
use cid::Cid;
use lru::LruCache;
use parking_lot::Mutex;
use std::io::{Read, Seek};
use std::sync::Arc;

pub trait CarReader: Read + Seek + Send + Sync + 'static {}
impl<X: Read + Seek + Send + Sync + 'static> CarReader for X {}

pub type ReaderKey = u64;
pub type ZstdFrameCache = Arc<Mutex<LruCache<(FrameOffset, ReaderKey), HashMap<Cid, Vec<u8>>>>>;

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

use std::io::{Read, Seek};

pub trait CarReader: Read + Seek + Send + Sync + 'static {}
impl<X: Read + Seek + Send + Sync + 'static> CarReader for X {}

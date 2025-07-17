// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use derive_more::{From, Into};
use get_size2::GetSize;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, From, Into)]
pub struct CidWrapper(pub Cid);
impl GetSize for CidWrapper {}

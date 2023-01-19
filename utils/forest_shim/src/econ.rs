// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::econ::TokenAmount as TokenAmount_v2;
use fvm_shared3::econ::TokenAmount as TokenAmount_v3;
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TokenAmount(TokenAmount_v3);

impl Deref for TokenAmount {
    type Target = TokenAmount_v3;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TokenAmount {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<TokenAmount_v3> for TokenAmount {
    fn from(other: TokenAmount_v3) -> Self {
        TokenAmount(other)
    }
}

impl From<TokenAmount_v2> for TokenAmount {
    fn from(other: TokenAmount_v2) -> Self {
        TokenAmount::from(TokenAmount_v3::from_atto(other.atto().clone()))
    }
}

impl From<TokenAmount> for TokenAmount_v3 {
    fn from(other: TokenAmount) -> Self {
        other.0
    }
}

impl From<TokenAmount> for TokenAmount_v2 {
    fn from(other: TokenAmount) -> TokenAmount_v2 {
        TokenAmount_v2::from_atto(other.atto().clone())
    }
}

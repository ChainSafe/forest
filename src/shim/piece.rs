// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::fvm_shared_latest::piece as piece_latest;
use cid::Cid;
use fvm_shared2::piece as piece_v2;
use fvm_shared3::piece as piece_v3;
use fvm_shared4::piece as piece_v4;
use serde::{Deserialize, Serialize};

/// Piece information for part or a whole file.
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(transparent)]
pub struct PieceInfo(piece_latest::PieceInfo);

impl PieceInfo {
    pub fn new(cid: Cid, size: PaddedPieceSize) -> Self {
        Self(piece_latest::PieceInfo {
            cid,
            size: size.into(),
        })
    }
}

impl From<PieceInfo> for piece_v4::PieceInfo {
    fn from(value: PieceInfo) -> Self {
        value.0
    }
}

impl From<piece_v4::PieceInfo> for PieceInfo {
    fn from(value: piece_v4::PieceInfo) -> Self {
        Self(value)
    }
}

impl From<PieceInfo> for piece_v2::PieceInfo {
    fn from(value: PieceInfo) -> Self {
        Self {
            size: PaddedPieceSize::from(value.0.size).into(),
            cid: value.0.cid,
        }
    }
}

impl From<piece_v2::PieceInfo> for PieceInfo {
    fn from(value: piece_v2::PieceInfo) -> Self {
        Self(piece_latest::PieceInfo {
            size: PaddedPieceSize::from(value.size).into(),
            cid: value.cid,
        })
    }
}

impl From<PieceInfo> for piece_v3::PieceInfo {
    fn from(value: PieceInfo) -> Self {
        Self {
            size: PaddedPieceSize::from(value.0.size).into(),
            cid: value.0.cid,
        }
    }
}

impl From<piece_v3::PieceInfo> for PieceInfo {
    fn from(value: piece_v3::PieceInfo) -> Self {
        Self(piece_latest::PieceInfo {
            size: PaddedPieceSize::from(value.size).into(),
            cid: value.cid,
        })
    }
}

/// Size of a piece in bytes with padding.
#[derive(PartialEq, Debug, Eq, Clone, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PaddedPieceSize(piece_latest::PaddedPieceSize);

impl From<PaddedPieceSize> for piece_v4::PaddedPieceSize {
    fn from(value: PaddedPieceSize) -> Self {
        value.0
    }
}

impl From<piece_v4::PaddedPieceSize> for PaddedPieceSize {
    fn from(value: piece_v4::PaddedPieceSize) -> Self {
        Self(value)
    }
}

impl From<PaddedPieceSize> for piece_v3::PaddedPieceSize {
    fn from(value: PaddedPieceSize) -> Self {
        Self(value.0 .0)
    }
}

impl From<piece_v3::PaddedPieceSize> for PaddedPieceSize {
    fn from(value: piece_v3::PaddedPieceSize) -> Self {
        Self(piece_latest::PaddedPieceSize(value.0))
    }
}

impl From<PaddedPieceSize> for piece_v2::PaddedPieceSize {
    fn from(value: PaddedPieceSize) -> Self {
        Self(value.0 .0)
    }
}

impl From<piece_v2::PaddedPieceSize> for PaddedPieceSize {
    fn from(value: piece_v2::PaddedPieceSize) -> Self {
        Self(piece_latest::PaddedPieceSize(value.0))
    }
}

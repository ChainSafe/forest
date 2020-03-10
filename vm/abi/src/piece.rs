// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;

type _UnpaddedPieceSize = u64;
type PaddedPieceSize = u64;

// TODO implement
pub struct PieceInfo {
    /// Size in nodes. For BLS12-381 (capacity 254 bits), must be >= 16. (16 * 8 = 128)
    pub size: PaddedPieceSize,
    /// Content identifier for piece
    pub cid: Cid,
}

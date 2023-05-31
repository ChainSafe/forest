// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
mod tests {
    use anyhow::*;
    use cid::{
        multihash::{Code::Sha2_256, Multihash, MultihashDigest},
        Cid,
    };
    use forest_shim::sector::compute_unsealed_sector_cid_v2;
    use fvm_shared::{commcid::FIL_COMMITMENT_UNSEALED, sector::RegisteredSealProof};
    use fvm_shared::{
        commcid::SHA2_256_TRUNC254_PADDED,
        piece::{PaddedPieceSize, PieceInfo},
    };
    use pretty_assertions::assert_eq;

    #[test]
    fn test_compute_unsealed_sector_cid_umpty_pieces() -> Result<()> {
        let pieces = vec![];

        let commd = compute_unsealed_sector_cid_v2(
            RegisteredSealProof::StackedDRG512MiBV1P1,
            pieces.as_slice(),
        )?;

        println!("commd: {commd}");
        assert_eq!(
            commd.to_string(),
            "baga6ea4seaqdsvqopmj2soyhujb72jza76t4wpq5fzifvm3ctz47iyytkewnubq"
        );

        Ok(())
    }

    /// Parity test in Go
    ///
    /// ```go
    /// package main
    ///
    /// import (
    ///     "fmt"
    ///     "math"
    ///
    ///     commp "github.com/filecoin-project/go-commp-utils/nonffi"
    ///     "github.com/filecoin-project/go-state-types/abi"
    ///     "github.com/ipfs/go-cid"
    ///     "github.com/multiformats/go-multihash"
    /// )
    ///
    /// const FIL_COMMITMENT_UNSEALED = 0xf101
    ///
    /// func main() {
    ///     pieces := make([]abi.PieceInfo, 0)
    ///     hash, err := multihash.FromHexString("9220209c2bef5edd10e6e042e766152c38b9bb9a107eb46193871e28da3c939bd30601")
    ///     if err != nil {
    ///         panic(err)
    ///     }
    ///     pieceCid := cid.NewCidV1(FIL_COMMITMENT_UNSEALED, hash)
    ///     fmt.Printf("pieceCid: %s\n", pieceCid.String())
    ///     // pieceCid: baga6ea4seaqjyk7pl3orbzxailtwmfjmhc43xgqqp22gde4hdyunupettpjqmai
    ///     pieceInfo := abi.PieceInfo{
    ///         Size:     abi.PaddedPieceSize(math.Pow(2, 10)),
    ///         PieceCID: pieceCid,
    ///     }
    ///     pieces = append(pieces, pieceInfo)
    ///     cid, err := commp.GenerateUnsealedCID(abi.RegisteredSealProof_StackedDrg512MiBV1_1, pieces)
    ///     if err != nil {
    ///         panic(err)
    ///     }
    ///     fmt.Println(cid)
    ///     // baga6ea4seaqjyk7pl3orbzxailtwmfjmhc43xgqqp22gde4hdyunupettpjqmai
    /// }
    /// ```
    #[test]
    fn test_compute_unsealed_sector_cid() -> Result<()> {
        let hash = Sha2_256.digest("test_compute_unsealed_sector_cid".as_bytes());
        let mh = Multihash::wrap(SHA2_256_TRUNC254_PADDED, hash.digest())?;
        println!("multihash hex: {}", hex::encode(mh.to_bytes()));
        let piece = PieceInfo {
            size: PaddedPieceSize(2_u64.pow(10)),
            cid: Cid::new_v1(FIL_COMMITMENT_UNSEALED, mh),
        };
        println!("piece cid: {}", piece.cid);
        assert_eq!(
            piece.cid.to_string(),
            "baga6ea4seaqjyk7pl3orbzxailtwmfjmhc43xgqqp22gde4hdyunupettpjqmai"
        );
        let pieces = vec![piece];

        let commd = compute_unsealed_sector_cid_v2(
            RegisteredSealProof::StackedDRG512MiBV1P1,
            pieces.as_slice(),
        )?;

        println!("commd: {commd}");
        assert_eq!(
            commd.to_string(),
            "baga6ea4seaqoebmodxmpe6nrd63rqey67bhby43u4mxyx2v2vpxnfv7zxdr6uaa"
        );

        Ok(())
    }
}

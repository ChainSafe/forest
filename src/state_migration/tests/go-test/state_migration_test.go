package main

import (
	"context"
	"fmt"
	"testing"

	"github.com/filecoin-project/go-state-types/abi"
	migration9Test "github.com/filecoin-project/go-state-types/builtin/v9/migration/test"
	"github.com/ipfs/go-cid"
	cbor "github.com/ipfs/go-ipld-cbor"
)

func TestStateMigrationNV17(t *testing.T) {
	startRoot := cid.MustParse("bafy2bzacedxtdhqjsrw2twioyaeomdk4z7umhgfv36vzrrotjb4woutphqgyg")
	newManifestCid := cid.MustParse("bafy2bzacedbedgynklc4dgpyxippkxmba2mgtw7ecntoneclsvvl4klqwuyyy")
	epoch := abi.ChainEpoch(16800)

	bs := migration9Test.NewSyncBlockStoreInMemory()
	ctx := context.Background()

	LoadCar(t, ctx, bs, fmt.Sprintf("../../../../target/actor_bundles/%s.car", newManifestCid))
	LoadCar(t, ctx, bs, fmt.Sprintf("../data/%s.car", startRoot))
	// Or LoadCompressedCar for .car.zst
	// LoadCompressedCar(t, ctx, bs, fmt.Sprintf("../data/%s.car.zst", startRoot))

	runStateMigration(t, ctx, cbor.NewCborStore(bs), startRoot, newManifestCid, epoch)
}

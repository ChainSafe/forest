package main

import (
	"context"
	"io"
	"os"
	"testing"

	"github.com/DataDog/zstd"
	"github.com/filecoin-project/go-state-types/abi"
	migration9 "github.com/filecoin-project/go-state-types/builtin/v9/migration"
	migration9Test "github.com/filecoin-project/go-state-types/builtin/v9/migration/test"
	"github.com/ipfs/go-cid"
	cbor "github.com/ipfs/go-ipld-cbor"
	"github.com/ipld/go-car"
)

func runStateMigration(
	t *testing.T,
	ctx context.Context,
	store cbor.IpldStore,
	startRoot cid.Cid,
	newManifestCid cid.Cid,
	epoch abi.ChainEpoch,
) {
	// Convert Rust state root to Go state root
	stateRoot := StateRoot{}
	if err := store.Get(ctx, startRoot, &stateRoot); err != nil {
		t.Error(err)
	}
	t.Logf("InStateRoot: Version: %d, Actors: %s, Info: %s\n", stateRoot.Version, stateRoot.Actors, stateRoot.Info)

	log := migration9Test.TestLogger{TB: t}
	cache := migration9.NewMemMigrationCache()
	outRoot, err := migration9.MigrateStateTree(ctx, store, newManifestCid, stateRoot.Actors, epoch, migration9.Config{MaxWorkers: 1}, log, cache)
	if err != nil {
		t.Error(err)
	}

	t.Logf("outRoot: %s", outRoot)
}

func LoadCompressedCar(t *testing.T, ctx context.Context, store *migration9Test.SyncBlockStoreInMemory, carFilePath string) {
	file, err := os.Open(carFilePath)
	if err != nil {
		t.Error(err)
	}
	reader := zstd.NewReader(file)
	defer reader.Close()
	loadCarWithReader(t, ctx, store, reader)
}

func LoadCar(t *testing.T, ctx context.Context, store *migration9Test.SyncBlockStoreInMemory, carFilePath string) {
	file, err := os.Open(carFilePath)
	if err != nil {
		t.Error(err)
	}
	loadCarWithReader(t, ctx, store, file)
}

func loadCarWithReader(t *testing.T, ctx context.Context, store *migration9Test.SyncBlockStoreInMemory, reader io.Reader) {
	carReader, err := car.NewCarReader(reader)
	if err != nil {
		t.Error(err)
	}
	for {
		if block, err := carReader.Next(); err == nil {
			store.Put(ctx, block)
		} else {
			break
		}
	}
}

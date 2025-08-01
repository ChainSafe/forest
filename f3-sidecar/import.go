package main

import (
	"bufio"
	"context"
	"os"

	"github.com/filecoin-project/go-f3/certstore"
	"github.com/filecoin-project/go-f3/manifest"
	"github.com/filecoin-project/go-jsonrpc"
	"github.com/ipfs/go-datastore"
	"github.com/ipfs/go-datastore/namespace"
	"github.com/ipfs/go-datastore/query"
	leveldb "github.com/ipfs/go-ds-leveldb"
)

func importSnap(ctx context.Context, rpcEndpoint string, f3Root string, snapshot string) error {
	logger.Infof("importing F3 snapshot at %s", snapshot)

	f3api := F3Api{}
	closer, err := jsonrpc.NewClient(ctx, rpcEndpoint, "F3", &f3api, nil)
	defer closer()
	rawNetworkName := waitRawNetworkName(ctx, &f3api)
	networkName := getNetworkName(rawNetworkName)
	m := Network2PredefinedManifestMappings[networkName]
	if m == nil {
		m2 := manifest.LocalDevnetManifest()
		m = &m2
		m.NetworkName = networkName
	}

	ds, err := getDatastore(f3Root)
	if err != nil {
		return err
	}
	defer ds.Close()
	dsBatcher := LevelDBBatchWriter{ds: ds}
	defer dsBatcher.Close()
	dsWrapper := namespace.Wrap(&dsBatcher, m.DatastorePrefix())
	defer dsWrapper.Close()

	f, err := os.Open(snapshot)
	if err != nil {
		return err
	}
	defer f.Close()
	return certstore.ImportSnapshotToDatastore(ctx, bufio.NewReader(f), dsWrapper)
}

type LevelDBBatchWriter struct {
	ds        *leveldb.Datastore
	batch     datastore.Batch
	batchSize int
}

func (w *LevelDBBatchWriter) Get(ctx context.Context, key datastore.Key) (value []byte, err error) {
	return w.ds.Get(ctx, key)
}

func (w *LevelDBBatchWriter) Has(ctx context.Context, key datastore.Key) (exists bool, err error) {
	return w.ds.Has(ctx, key)
}

func (w *LevelDBBatchWriter) GetSize(ctx context.Context, key datastore.Key) (size int, err error) {
	return w.ds.GetSize(ctx, key)
}

func (w *LevelDBBatchWriter) Query(ctx context.Context, q query.Query) (query.Results, error) {
	return w.ds.Query(ctx, q)
}

func (w *LevelDBBatchWriter) Close() error {
	return w.ds.Close()
}

func (w *LevelDBBatchWriter) Sync(ctx context.Context, prefix datastore.Key) error {
	if err := w.Flush(ctx); err != nil {
		return err
	}
	return w.ds.Sync(ctx, prefix)
}

func (w *LevelDBBatchWriter) Put(ctx context.Context, key datastore.Key, value []byte) error {
	if w.batch == nil {
		batch, err := w.ds.Batch(ctx)
		if err != nil {
			return err
		}
		w.batch = batch
	}

	if w.batchSize >= 1024 {
		if err := w.Flush(ctx); err != nil {
			return err
		}
	}

	w.batchSize += 1
	return w.batch.Put(ctx, key, value)

}

func (w *LevelDBBatchWriter) Delete(ctx context.Context, key datastore.Key) error {
	return w.ds.Delete(ctx, key)
}

func (w *LevelDBBatchWriter) Flush(ctx context.Context) error {
	if w.batch != nil {
		if err := w.batch.Commit(ctx); err != nil {
			return err
		}
		batch, err := w.ds.Batch(ctx)
		if err != nil {
			return err
		}
		w.batch = batch
		w.batchSize = 0
	}
	return nil
}

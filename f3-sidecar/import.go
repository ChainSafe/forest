package main

import (
	"bufio"
	"context"
	"os"

	"github.com/filecoin-project/go-f3/certstore"
)

func importSnap(ctx context.Context, f3Root string, snapshot string) error {
	logger.Infof("importing F3 snapshot at %s", snapshot)
	ds, err := getDatastore(f3Root)
	if err != nil {
		return err
	}
	defer ds.Close()
	f, err := os.Open(snapshot)
	if err != nil {
		return err
	}
	defer f.Close()
	certstore.ImportSnapshotToDatastore(ctx, bufio.NewReader(f), ds)
	return nil
}

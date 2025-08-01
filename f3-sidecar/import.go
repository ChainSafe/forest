package main

import (
	"bufio"
	"context"
	"os"

	"github.com/filecoin-project/go-f3/certstore"
	"github.com/filecoin-project/go-f3/manifest"
	"github.com/filecoin-project/go-jsonrpc"
	"github.com/ipfs/go-datastore/namespace"
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
	dsWrapper := namespace.Wrap(ds, m.DatastorePrefix())
	defer dsWrapper.Close()

	f, err := os.Open(snapshot)
	if err != nil {
		return err
	}
	defer f.Close()
	return certstore.ImportSnapshotToDatastore(ctx, bufio.NewReader(f), dsWrapper)
}

package main

import (
	"context"
	"path/filepath"
	"time"

	"github.com/filecoin-project/go-f3/gpbft"
	"github.com/ipfs/go-cid"
	leveldb "github.com/ipfs/go-ds-leveldb"
)

var CID_UNDEF_RUST = cid.MustParse("baeaaaaa")

func isCidDefined(c cid.Cid) bool {
	return c.Defined() && c != CID_UNDEF_RUST
}

func getDatastore(f3Root string) (*leveldb.Datastore, error) {
	return leveldb.NewDatastore(filepath.Join(f3Root, "db"), nil)
}

func waitRawNetworkName(ctx context.Context, f3api *F3Api) string {
	for {
		rawNetwork, err := f3api.GetRawNetworkName(ctx)
		if err != nil {
			logger.Warnln("waiting for Forest RPC server")
			time.Sleep(5 * time.Second)
			continue
		}

		logger.Infoln("Forest RPC server is online")
		return rawNetwork
	}
}

func getNetworkName(rawNetworkName string) gpbft.NetworkName {
	networkName := gpbft.NetworkName(rawNetworkName)
	// See <https://github.com/filecoin-project/lotus/blob/v1.33.1/chain/lf3/config.go#L65>
	// Use "filecoin" as the network name on mainnet, otherwise use the network name. Yes,
	// mainnet is called testnetnet in state.
	if networkName == "testnetnet" {
		networkName = "filecoin"
	}
	return networkName
}

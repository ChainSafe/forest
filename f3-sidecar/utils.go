package main

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"time"

	"github.com/filecoin-project/go-f3/gpbft"
	"github.com/ipfs/go-cid"
	leveldb "github.com/ipfs/go-ds-leveldb"
	logging "github.com/ipfs/go-log/v2"
	"github.com/libp2p/go-libp2p/gologshim"
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

func setLogLevel(name string, level string) error {
	if err := logging.SetLogLevel(name, level); err != nil {
		return fmt.Errorf("%s %w", name, err)
	}
	return nil
}

func setLogLevels() error {
	// Route all slog logs through go-log
	slog.SetDefault(slog.New(logging.SlogHandler()))

	// Connect go-libp2p to go-log
	gologshim.SetDefaultHandler(logging.SlogHandler())

	logging.SetAllLoggers(logging.LevelInfo)
	if err := setLogLevel("dht", "error"); err != nil {
		return err
	}
	// Always mute RtRefreshManager because it breaks terminals
	if err := setLogLevel("dht/RtRefreshManager", "fatal"); err != nil {
		return err
	}
	if err := setLogLevel("f3/sidecar", "debug"); err != nil {
		return err
	}
	return nil
}

func checkError(err error) {
	if err != nil {
		panic(err)
	}
}

// To avoid potential panics
// See <https://github.com/ChainSafe/forest/pull/4636#issuecomment-2306500753>
func setGoDebugEnv() {
	err := os.Setenv("GODEBUG", "invalidptr=0,cgocheck=0")
	checkError(err)
}

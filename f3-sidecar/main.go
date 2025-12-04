package main

import (
	"context"
	"flag"

	logging "github.com/ipfs/go-log/v2"
)

var logger = logging.Logger("f3/sidecar")

func main() {
	checkError(setLogLevels())

	var rpcEndpoint string
	flag.StringVar(&rpcEndpoint, "rpc", "http://127.0.0.1:2345/rpc/v1", "forest RPC endpoint")
	var jwt string
	flag.StringVar(&jwt, "jwt", "", "the JWT token for invoking forest RPC methods that require WRITE and SIGN permission")
	var f3RpcEndpoint string
	flag.StringVar(&f3RpcEndpoint, "f3-rpc", "127.0.0.1:23456", "The RPC endpoint F3 sidecar listens on")
	var initialPowerTable string
	flag.StringVar(&initialPowerTable, "initial-power-table", "", "The CID of the initial power table")
	var bootstrapEpoch int64
	flag.Int64Var(&bootstrapEpoch, "bootstrap", -1, "F3 bootstrap epoch")
	var finality int64
	flag.Int64Var(&finality, "finality", 900, "chain finality epochs")
	var f3Root string
	flag.StringVar(&f3Root, "root", "f3-data", "path to the f3 data directory")
	var snapshotPath string
	flag.StringVar(&snapshotPath, "snapshot", "", "path to the f3 snapshot file")

	flag.Parse()

	ctx := context.Background()

	if len(snapshotPath) > 0 {
		if err := importSnap(ctx, rpcEndpoint, f3Root, snapshotPath); err != nil {
			panic(err)
		}
	}

	err := run(ctx, rpcEndpoint, jwt, f3RpcEndpoint, initialPowerTable, bootstrapEpoch, finality, f3Root)
	if err != nil {
		panic(err)
	}
}

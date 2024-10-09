package main

import (
	"context"
	"flag"

	logging "github.com/ipfs/go-log/v2"
)

var logger = logging.Logger("f3/sidecar")

func main() {
	logging.SetAllLoggers(logging.LevelError)
	if err := logging.SetLogLevel("f3/sidecar", "debug"); err != nil {
		panic(err)
	}
	if err := logging.SetLogLevel("f3", "debug"); err != nil {
		panic(err)
	}

	var rpcEndpoint string
	flag.StringVar(&rpcEndpoint, "rpc", "http://127.0.0.1:2345/rpc/v1", "forest RPC endpoint")
	var f3RpcEndpoint string
	flag.StringVar(&f3RpcEndpoint, "f3-rpc", "127.0.0.1:23456", "The RPC endpoint F3 sidecar listens on")
	var initialPowerTable string
	flag.StringVar(&initialPowerTable, "initial-power-table", "", "The CID of the initial power table")
	var finality int64
	flag.Int64Var(&finality, "finality", 900, "chain finality epochs")
	var root string
	flag.StringVar(&root, "root", "f3-data", "path to the f3 data directory")
	var manifestServer string
	flag.StringVar(&manifestServer, "manifest-server", "", "the peer id of the dynamic manifest server")
	flag.Parse()

	ctx := context.Background()

	err := run(ctx, rpcEndpoint, f3RpcEndpoint, initialPowerTable, finality, root, manifestServer)
	if err != nil {
		panic(err)
	}
}

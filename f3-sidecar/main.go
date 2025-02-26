package main

import (
	"context"
	"flag"

	logging "github.com/ipfs/go-log/v2"
)

var logger = logging.Logger("f3/sidecar")

func main() {
	logging.SetAllLoggers(logging.LevelInfo)
	if err := logging.SetLogLevel("dht", "error"); err != nil {
		panic(err)
	}
	if err := logging.SetLogLevel("dht/RtRefreshManager", "warn"); err != nil {
		panic(err)
	}
	if err := logging.SetLogLevel("net/identify", "error"); err != nil {
		panic(err)
	}
	if err := logging.SetLogLevel("pubsub", "warn"); err != nil {
		panic(err)
	}
	if err := logging.SetLogLevel("f3/sidecar", "debug"); err != nil {
		panic(err)
	}

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
	var root string
	flag.StringVar(&root, "root", "f3-data", "path to the f3 data directory")
	var contract_poll_interval uint64
	flag.Uint64Var(&contract_poll_interval, "contract-poll-interval", 900, "contract manifest poll interval seconds")

	flag.Parse()

	ctx := context.Background()

	err := run(ctx, rpcEndpoint, jwt, f3RpcEndpoint, initialPowerTable, bootstrapEpoch, finality, root, contract_poll_interval)
	if err != nil {
		panic(err)
	}
}

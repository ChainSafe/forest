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
	var finality int64
	flag.Int64Var(&finality, "finality", 900, "chain finality epochs")
	var db string
	flag.StringVar(&db, "db", "f3-db", "path to the f3 database")
	var manifestServer string
	flag.StringVar(&manifestServer, "manifest-server", "", "the peer id of the dynamic manifest server")
	flag.Parse()

	ctx := context.Background()

	err := run(ctx, rpcEndpoint, f3RpcEndpoint, finality, db, manifestServer)
	if err != nil {
		panic(err)
	}
}

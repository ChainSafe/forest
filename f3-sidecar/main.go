package main

import (
	"context"
	"flag"

	logging "github.com/ipfs/go-log/v2"
)

var logger = logging.Logger("f3-sidecar")

func main() {
	logging.SetAllLoggers(logging.LevelError)
	if err := logging.SetLogLevel("f3-sidecar", "debug"); err != nil {
		panic(err)
	}
	if err := logging.SetLogLevel("f3", "debug"); err != nil {
		panic(err)
	}

	var rpcEndpoint string
	flag.StringVar(&rpcEndpoint, "rpc", "http://127.0.0.1:2345/rpc/v1", "forest RPC endpoint")
	var finality int64
	flag.Int64Var(&finality, "finality", 900, "chain finality epochs")
	var db string
	flag.StringVar(&db, "db", "f3-db", "path to the f3 database")
	flag.Parse()

	ctx := context.Background()

	err := run(ctx, rpcEndpoint, finality, db)
	if err != nil {
		panic(err)
	}
}

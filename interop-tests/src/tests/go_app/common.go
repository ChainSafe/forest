package main

import "os"

const ListenAddr = "/ip4/127.0.0.1/tcp/0"

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

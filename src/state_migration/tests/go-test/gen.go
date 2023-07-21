package main

import (
	gen "github.com/whyrusleeping/cbor-gen"
)

func main() {
	if err := gen.WriteTupleEncodersToFile("./cbor_gen.go", "main",
		StateRoot{},
	); err != nil {
		panic(err)
	}
}

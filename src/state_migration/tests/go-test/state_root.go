package main

import "github.com/ipfs/go-cid"

type StateRoot struct {
	Version uint64
	Actors  cid.Cid
	Info    cid.Cid
}

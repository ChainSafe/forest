package main

import "github.com/ipfs/go-cid"

var CID_UNDEF_RUST = cid.MustParse("baeaaaaa")

func isCidDefined(c cid.Cid) bool {
	return c.Defined() && c != CID_UNDEF_RUST
}

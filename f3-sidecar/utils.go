package main

import (
	"path/filepath"

	"github.com/ipfs/go-cid"
	leveldb "github.com/ipfs/go-ds-leveldb"
)

var CID_UNDEF_RUST = cid.MustParse("baeaaaaa")

func isCidDefined(c cid.Cid) bool {
	return c.Defined() && c != CID_UNDEF_RUST
}

func getDatastore(f3Root string) (*leveldb.Datastore, error) {
	return leveldb.NewDatastore(filepath.Join(f3Root, "db"), nil)
}

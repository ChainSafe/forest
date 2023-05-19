package test

import (
	"context"
	"testing"

	"github.com/filecoin-project/go-state-types/builtin"
	adt8 "github.com/filecoin-project/go-state-types/builtin/v8/util/adt"
	adt9 "github.com/filecoin-project/go-state-types/builtin/v9/util/adt"
	cbor "github.com/ipfs/go-ipld-cbor"
)

func TestStoreEmptyMap(t *testing.T) {
	ctx := context.Background()
	store := cbor.NewMemCborStore()
	adtStore := adt8.WrapStore(ctx, store)
	emptyMapCid, err := adt9.StoreEmptyMap(adtStore, builtin.DefaultHamtBitwidth)
	if err != nil {
		t.Error(err)
	}
	if emptyMapCid.String() != "bafy2bzaceamp42wmmgr2g2ymg46euououzfyck7szknvfacqscohrvaikwfay" {
		t.Error(emptyMapCid.String())
	}
}

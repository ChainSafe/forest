package main

import (
	"context"
	"encoding/json"
	"fmt"
	"testing"

	"github.com/filecoin-project/go-f3/ec"
	"github.com/stretchr/testify/require"
)

var (
	EC  ec.Backend
	ctx = context.Background()
)

func init() {
	ec, err := NewForestEC(ctx, "http://127.0.0.1:2345/rpc/v1", "")
	if err != nil {
		panic(err)
	}
	EC = &ec
}

func TestGetTipsetByEpoch(t *testing.T) {
	head, err := EC.GetHead(ctx)
	require.NoError(t, err)
	ts, err := EC.GetTipsetByEpoch(ctx, head.Epoch()-10)
	require.NoError(t, err)
	fmt.Printf("GetTipsetByEpoch: %s\n", ts)
}

func TestGetHead(t *testing.T) {
	head, err := EC.GetHead(ctx)
	require.NoError(t, err)
	fmt.Printf("GetHead: %s\n", head)
}

func TestGetTipset(t *testing.T) {
	head, err := EC.GetHead(ctx)
	require.NoError(t, err)
	ts, err := EC.GetTipset(ctx, head.Key())
	require.NoError(t, err)
	require.Equal(t, head, ts)
	fmt.Printf("GetTipset: %s\n", ts)
}

func TestGetParent(t *testing.T) {
	head, err := EC.GetHead(ctx)
	require.NoError(t, err)
	ts, err := EC.GetParent(ctx, head)
	require.NoError(t, err)
	require.NotEqual(t, head, ts)
	fmt.Printf("GetParent: %s\n", ts)
}

func TestGetPowerTable(t *testing.T) {
	head, err := EC.GetHead(ctx)
	require.NoError(t, err)
	pt, err := EC.GetPowerTable(ctx, head.Key())
	require.NoError(t, err)
	ptJsonBytes, err := json.Marshal(&pt)
	require.NoError(t, err)
	fmt.Printf("GetPowerTable: %s\n", string(ptJsonBytes))
}

func TestFinalize(t *testing.T) {
	head, err := EC.GetHead(ctx)
	require.NoError(t, err)
	err = EC.Finalize(ctx, head.Key())
	require.ErrorContains(t, err, "unable to finalize tipset, jwt is not provided")
}

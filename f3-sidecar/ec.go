package main

import (
	"context"

	"github.com/filecoin-project/go-f3/ec"
	"github.com/filecoin-project/go-f3/gpbft"
	"github.com/filecoin-project/go-jsonrpc"
)

type ForestEC struct {
	rpcEndpoint string
	f3api       F3Api
	closer      jsonrpc.ClientCloser
}

func NewForestEC(rpcEndpoint string) (ForestEC, error) {
	f3api := F3Api{}
	closer, err := jsonrpc.NewClient(context.Background(), rpcEndpoint, "F3", &f3api, nil)
	if err != nil {
		return ForestEC{}, err
	}
	return ForestEC{rpcEndpoint, f3api, closer}, nil
}

func (ec *ForestEC) Close() {
	ec.closer()
}

func (ec *ForestEC) GetTipsetByEpoch(ctx context.Context, epoch int64) (ec.TipSet, error) {
	return ec.f3api.GetTipsetByEpoch(ctx, epoch)
}

func (ec *ForestEC) GetTipset(ctx context.Context, tsk gpbft.TipSetKey) (ec.TipSet, error) {
	return ec.f3api.GetTipset(ctx, tsk)
}

func (ec *ForestEC) GetHead(ctx context.Context) (ec.TipSet, error) {
	return ec.f3api.GetHead(ctx)
}

func (ec *ForestEC) GetParent(ctx context.Context, ts ec.TipSet) (ec.TipSet, error) {
	return ec.f3api.GetParent(ctx, ts.Key())
}

func (ec *ForestEC) GetPowerTable(ctx context.Context, tsk gpbft.TipSetKey) (gpbft.PowerEntries, error) {
	return ec.f3api.GetPowerTable(ctx, tsk)
}

func (ec *ForestEC) Sign(ctx context.Context, sender gpbft.PubKey, msg []byte) ([]byte, error) {
	signature, err := ec.f3api.SignMessage(ctx, sender, msg)
	if err != nil {
		return nil, err
	}
	return signature.Data, err
}

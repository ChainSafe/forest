package main

import (
	"context"
	"fmt"
	"net/http"

	"github.com/filecoin-project/go-f3/ec"
	"github.com/filecoin-project/go-f3/gpbft"
	"github.com/filecoin-project/go-jsonrpc"
)

type ForestEC struct {
	rpcEndpoint   string
	isJwtProvided bool
	f3api         `F3`Api
	closer        jsonrpc.ClientCloser
}

func NewForestEC(ctx context.Context, rpcEndpoint, jwt string) (ForestEC, error) {
	f3api := `F3`Api{}
	headers := make(http.Header)
	isJwtProvided := len(jwt) > 0
	if isJwtProvided {
		headers.Add("Authorization", fmt.Sprintf("Bearer %s", jwt))
	}
	closer, err := jsonrpc.NewClient(ctx, rpcEndpoint, "`F3`", &f3api, headers)
	if err != nil {
		return ForestEC{}, err
	}
	return ForestEC{rpcEndpoint, isJwtProvided, f3api, closer}, nil
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

func (ec *ForestEC) Finalize(ctx context.Context, tsk gpbft.TipSetKey) error {
	if !ec.isJwtProvided {
		return fmt.Errorf("unable to finalize tipset, jwt is not provided")
	}
	return ec.f3api.Finalize(ctx, tsk)
}

func (ec *ForestEC) Sign(ctx context.Context, sender gpbft.PubKey, msg []byte) ([]byte, error) {
	if !ec.isJwtProvided {
		return nil, fmt.Errorf("unable to sign messages, jwt is not provided")
	}
	signature, err := ec.f3api.SignMessage(ctx, sender, msg)
	if err != nil {
		return nil, err
	}
	return signature.Data, err
}

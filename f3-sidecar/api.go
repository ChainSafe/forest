package main

import (
	"context"

	"github.com/filecoin-project/go-f3/gpbft"
)

type F3Api struct {
	GetTipsetByEpoch func(context.Context, int64) (TipSet, error)
	GetTipset        func(context.Context, gpbft.TipSetKey) (TipSet, error)
	GetHead          func(context.Context) (TipSet, error)
	GetParent        func(context.Context, gpbft.TipSetKey) (TipSet, error)
	GetPowerTable    func(context.Context, gpbft.TipSetKey) (gpbft.PowerEntries, error)
}

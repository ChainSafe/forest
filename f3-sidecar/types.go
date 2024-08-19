package main

import (
	"encoding/json"
	"fmt"
	"time"

	"github.com/filecoin-project/go-f3/gpbft"
)

type TipSet struct {
	TsKey       []byte `json:"key"`
	TsBeacon    []byte `json:"beacon"`
	TsEpoch     int64  `json:"epoch"`
	TsTimestamp int64  `json:"timestamp"`
}

func (ts TipSet) Key() gpbft.TipSetKey {
	return gpbft.TipSetKey(ts.TsKey)
}

func (ts TipSet) Beacon() []byte {
	return ts.TsBeacon
}

func (ts TipSet) Epoch() int64 {
	return ts.TsEpoch
}

func (ts TipSet) Timestamp() time.Time {
	return time.Unix(ts.TsTimestamp, 0)
}

func (ts TipSet) String() string {
	bytes, err := json.Marshal(&ts)
	if err != nil {
		return fmt.Sprintf("%s", err)
	}
	return string(bytes)
}

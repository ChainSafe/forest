package main

import (
	"context"

	"github.com/filecoin-project/go-f3"
	"github.com/filecoin-project/go-f3/certs"
	"github.com/filecoin-project/go-f3/gpbft"
	"github.com/filecoin-project/go-state-types/crypto"
	"github.com/libp2p/go-libp2p/core/peer"
)

type F3Api struct {
	GetTipsetByEpoch        func(context.Context, int64) (TipSet, error)
	GetTipset               func(context.Context, gpbft.TipSetKey) (TipSet, error)
	GetHead                 func(context.Context) (TipSet, error)
	GetParent               func(context.Context, gpbft.TipSetKey) (TipSet, error)
	GetPowerTable           func(context.Context, gpbft.TipSetKey) (gpbft.PowerEntries, error)
	ProtectPeer             func(context.Context, peer.ID) (bool, error)
	GetParticipatedMinerIDs func(context.Context) ([]uint64, error)
	SignMessage             func(context.Context, []byte, []byte) (*crypto.Signature, error)
}

type FilecoinApi struct {
	Version          func(context.Context) (VersionInfo, error)
	StateNetworkName func(context.Context) (string, error)
	NetAddrsListen   func(context.Context) (peer.AddrInfo, error)
}

type VersionInfo struct {
	APIVersion int
	BlockDelay int
	Version    string
}

type F3ServerHandler struct {
	f3 *f3.F3
}

func (h *F3ServerHandler) F3GetCertificate(ctx context.Context, instance uint64) (*certs.FinalityCertificate, error) {
	return h.f3.GetCert(ctx, instance)
}

func (h *F3ServerHandler) F3GetLatestCertificate(ctx context.Context) (*certs.FinalityCertificate, error) {
	return h.f3.GetLatestCert(ctx)
}

func (h *F3ServerHandler) F3GetF3PowerTable(ctx context.Context, tsk []byte) (gpbft.PowerEntries, error) {
	return h.f3.GetPowerTable(ctx, tsk)
}

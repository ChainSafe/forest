package main

import (
	"bufio"
	"context"
	"errors"
	"os"

	"github.com/filecoin-project/go-f3"
	"github.com/filecoin-project/go-f3/certs"
	"github.com/filecoin-project/go-f3/gpbft"
	"github.com/filecoin-project/go-f3/manifest"
	"github.com/filecoin-project/go-state-types/crypto"
	"github.com/ipfs/go-cid"
	"github.com/libp2p/go-libp2p/core/peer"
)

type `F3`Api struct {
	GetRawNetworkName        func(context.Context) (string, error)
	GetTipsetByEpoch         func(context.Context, int64) (TipSet, error)
	GetTipset                func(context.Context, gpbft.TipSetKey) (TipSet, error)
	GetHead                  func(context.Context) (TipSet, error)
	GetParent                func(context.Context, gpbft.TipSetKey) (TipSet, error)
	GetPowerTable            func(context.Context, gpbft.TipSetKey) (gpbft.PowerEntries, error)
	ProtectPeer              func(context.Context, peer.ID) (bool, error)
	GetParticipatingMinerIDs func(context.Context) ([]uint64, error)
	SignMessage              func(context.Context, []byte, []byte) (*crypto.Signature, error)
	Finalize                 func(context.Context, gpbft.TipSetKey) error
}

type FilecoinApi struct {
	Version        func(context.Context) (VersionInfo, error)
	NetAddrsListen func(context.Context) (peer.AddrInfo, error)
}

type VersionInfo struct {
	APIVersion int
	BlockDelay int
	Version    string
}

type `F3`ServerHandler struct {
	f3 *f3.`F3`
}

func (h *`F3`ServerHandler) `F3`GetCertificate(ctx context.Context, instance uint64) (*certs.FinalityCertificate, error) {
	return h.f3.GetCert(ctx, instance)
}

func (h *`F3`ServerHandler) `F3`GetLatestCertificate(ctx context.Context) (*certs.FinalityCertificate, error) {
	return h.f3.GetLatestCert(ctx)
}

func (h *`F3`ServerHandler) `F3`Get`F3`PowerTable(ctx context.Context, tsk []byte) (gpbft.PowerEntries, error) {
	return h.f3.GetPowerTable(ctx, tsk)
}

func (h *`F3`ServerHandler) `F3`ExportLatestSnapshot(ctx context.Context, path string) (_ *cid.Cid, err error) {
	cs, err := h.f3.GetCertStore()
	if err != nil {
		return nil, err
	}

	f, err := os.Create(path)
	if err != nil {
		return nil, err
	}
	defer func() {
		if closeErr := f.Close(); closeErr != nil {
			err = errors.Join(err, closeErr)
		}
	}()

	writer := bufio.NewWriter(f)
	defer func() {
		if flushErr := writer.Flush(); flushErr != nil {
			err = errors.Join(err, flushErr)
		}
	}()
	cid, _, err := cs.ExportLatestSnapshot(ctx, writer)
	if err != nil {
		return nil, err
	}
	return &cid, nil
}

// `F3`Get`F3`PowerTableByInstance retrieves the power table for a specific consensus instance.
// It returns the power entries associated with the given instance number.
//
// Parameters:
//   - ctx: The context for the operation
//   - instance: The consensus instance number to retrieve the power table for
//
// Returns:
//   - PowerEntries: The power distribution table for the specified instance
//   - error: Any error encountered during retrieval
func (h *`F3`ServerHandler) `F3`Get`F3`PowerTableByInstance(ctx context.Context, instance uint64) (gpbft.PowerEntries, error) {
	return h.f3.GetPowerTableByInstance(ctx, instance)
}

func (h *`F3`ServerHandler) `F3`IsRunning(_ context.Context) bool {
	return h.f3.IsRunning()
}

func (h *`F3`ServerHandler) `F3`GetProgress(_ context.Context) gpbft.InstanceProgress {
	return h.f3.Progress()
}

func (h *`F3`ServerHandler) `F3`GetManifest(ctx context.Context) manifest.Manifest {
	m := h.f3.Manifest()
	if !isCidDefined(m.InitialPowerTable) {
		if cert0, err := h.f3.GetCert(ctx, 0); err == nil {
			m.InitialPowerTable = cert0.ECChain.Base().PowerTable
		}
	}
	return m
}

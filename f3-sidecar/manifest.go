package main

import (
	"context"
	_ "embed"
	"encoding/json"
	"time"

	"github.com/filecoin-project/go-f3/gpbft"
	"github.com/filecoin-project/go-f3/manifest"
	"github.com/ipfs/go-cid"
)

var Network2PredefinedManifestMappings map[gpbft.NetworkName]*manifest.Manifest = make(map[gpbft.NetworkName]*manifest.Manifest)

func init() {
	for _, bytes := range [][]byte{F3ManifestBytes2K, F3ManifestBytesButterfly, F3ManifestBytesCalibnet, F3ManifestBytesMainnet} {
		m := loadManifest(bytes)
		Network2PredefinedManifestMappings[m.NetworkName] = m
	}
}

type ContractManifestProvider struct {
	started                 *bool
	pollInterval            time.Duration
	currentManifest         *manifest.Manifest
	staticInitialPowerTable cid.Cid
	f3Api                   *F3Api
	ch                      chan *manifest.Manifest
}

func NewContractManifestProvider(staticManifest *manifest.Manifest, contract_manifest_poll_interval_seconds uint64, f3Api *F3Api) (*ContractManifestProvider, error) {
	started := false
	pollInterval := time.Duration(contract_manifest_poll_interval_seconds) * time.Second
	var staticInitialPowerTable cid.Cid = cid.Undef
	if staticManifest != nil && isCidDefined(staticManifest.InitialPowerTable) {
		staticInitialPowerTable = staticManifest.InitialPowerTable
	}
	p := ContractManifestProvider{
		started:                 &started,
		pollInterval:            pollInterval,
		currentManifest:         nil,
		staticInitialPowerTable: staticInitialPowerTable,
		f3Api:                   f3Api,
		ch:                      make(chan *manifest.Manifest),
	}
	return &p, nil
}

func (p *ContractManifestProvider) Update(m *manifest.Manifest) {
	err := m.Validate()
	if err == nil {
		p.currentManifest = m
		p.ch <- m
	} else {
		logger.Warnf("Invalid manifest, skip updating, %s\n", err)
	}
}

func (p *ContractManifestProvider) Start(ctx context.Context) error {
	if *p.started {
		logger.Warnln("ContractManifestProvider has already been started")
		return nil
	}

	started := true
	p.started = &started
	go func() {
		for started && ctx.Err() == nil {
			logger.Debugf("Polling manifest from contract...\n")
			if m, err := p.f3Api.GetManifestFromContract(ctx); err == nil {
				if m != nil {
					if !isCidDefined(m.InitialPowerTable) {
						m.InitialPowerTable = p.staticInitialPowerTable
					}
					if !m.Equal(p.currentManifest) {
						logger.Infoln("Successfully polled manifest from contract, updating...")
						p.Update(m)
					} else {
						logger.Infoln("Successfully polled unchanged manifest from contract")
					}
				}

			} else {
				logger.Warnf("failed to get manifest from contract: %s\n", err)
			}
			time.Sleep(p.pollInterval)
		}
	}()

	return nil
}
func (p *ContractManifestProvider) Stop(context.Context) error {
	*p.started = false
	return nil
}
func (p *ContractManifestProvider) ManifestUpdates() <-chan *manifest.Manifest { return p.ch }

//go:embed f3manifest_2k.json
var F3ManifestBytes2K []byte

//go:embed f3manifest_butterfly.json
var F3ManifestBytesButterfly []byte

//go:embed f3manifest_calibnet.json
var F3ManifestBytesCalibnet []byte

//go:embed f3manifest_mainnet.json
var F3ManifestBytesMainnet []byte

func loadManifest(bytes []byte) *manifest.Manifest {
	var m manifest.Manifest
	if err := json.Unmarshal(bytes, &m); err != nil {
		logger.Panicf("failed to unmarshal F3 manifest: %s", err)
	}
	if err := m.Validate(); err != nil {
		logger.Panicf("invalid F3 manifest: %s", err)
	}
	return &m
}

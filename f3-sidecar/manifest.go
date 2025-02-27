package main

import (
	"context"
	"time"

	"github.com/filecoin-project/go-f3/manifest"
)

type ContractManifestProvider struct {
	started         *bool
	pollInterval    time.Duration
	initialManifest *manifest.Manifest
	currentManifest *manifest.Manifest
	f3Api           *F3Api
	ch              chan *manifest.Manifest
}

func NewContractManifestProvider(initialValue *manifest.Manifest, contract_manifest_poll_interval_seconds uint64, f3Api *F3Api) (*ContractManifestProvider, error) {
	started := false
	pollInterval := time.Duration(contract_manifest_poll_interval_seconds) * time.Second
	p := ContractManifestProvider{
		started:         &started,
		pollInterval:    pollInterval,
		initialManifest: initialValue,
		currentManifest: nil,
		f3Api:           f3Api,
		ch:              make(chan *manifest.Manifest),
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
			if p.currentManifest == nil && p.initialManifest != nil {
				p.Update(p.initialManifest)
				p.initialManifest = nil
			}
			logger.Debugf("Polling manifest from contract...\n")
			m, err := p.f3Api.GetManifestFromContract(ctx)
			if err == nil {
				if m != nil {
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

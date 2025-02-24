package main

import (
	"context"
	"time"

	"github.com/filecoin-project/go-f3/manifest"
)

type ContractManifestProvider struct {
	started         *bool
	pollInterval    time.Duration
	currentManifest *manifest.Manifest
	f3Api           *F3Api
	ch              chan *manifest.Manifest
}

func NewContractManifestProvider(initialValue *manifest.Manifest, contract_manifest_poll_interval_seconds uint64, f3Api *F3Api) (*ContractManifestProvider, error) {
	if err := initialValue.Validate(); err != nil {
		return nil, err
	}
	started := false
	pollInterval := time.Duration(contract_manifest_poll_interval_seconds) * time.Second
	p := ContractManifestProvider{
		started:         &started,
		pollInterval:    pollInterval,
		currentManifest: initialValue,
		f3Api:           f3Api,
		ch:              make(chan *manifest.Manifest),
	}
	p.Update(initialValue)
	return &p, nil
}

func (p *ContractManifestProvider) Update(m *manifest.Manifest) {
	p.currentManifest = m
	p.ch <- m
}

func (p *ContractManifestProvider) Start(ctx context.Context) error {
	started := true
	p.started = &started
	go func() {
		for started && ctx.Err() == nil {
			logger.Infof("Polling manifest from contract...\n")
			m, err := p.f3Api.GetManifestFromContract(ctx)
			if err == nil {
				if m != nil {
					if !m.Equal(p.currentManifest) {
						logger.Infof("Successfully polled manifest from contract, updating...\n")
						p.Update(m)
					} else {
						logger.Infof("Successfully polled unchanged manifest from contract\n")
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

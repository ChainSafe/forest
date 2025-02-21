package main

import (
	"context"

	"github.com/filecoin-project/go-f3/manifest"
)

type ForestManifestProvider struct {
	ch chan *manifest.Manifest
}

func NewForestManifestProvider(initialValue *manifest.Manifest) (*ForestManifestProvider, error) {
	if err := initialValue.Validate(); err != nil {
		return nil, err
	}
	p := ForestManifestProvider{ch: make(chan *manifest.Manifest)}
	p.Update(initialValue)
	return &p, nil
}

func (p *ForestManifestProvider) Update(m *manifest.Manifest) {
	p.ch <- m
}

func (p *ForestManifestProvider) Start(context.Context) error                { return nil }
func (p *ForestManifestProvider) Stop(context.Context) error                 { return nil }
func (p *ForestManifestProvider) ManifestUpdates() <-chan *manifest.Manifest { return p.ch }

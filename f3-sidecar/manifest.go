package main

import (
	_ "embed"
	"encoding/json"

	"github.com/filecoin-project/go-f3/gpbft"
	"github.com/filecoin-project/go-f3/manifest"
)

var Network2PredefinedManifestMappings map[gpbft.NetworkName]*manifest.Manifest = make(map[gpbft.NetworkName]*manifest.Manifest)

func init() {
	for _, bytes := range [][]byte{F3ManifestBytes2K, F3ManifestBytesButterfly, F3ManifestBytesCalibnet, F3ManifestBytesMainnet} {
		m := loadManifest(bytes)
		Network2PredefinedManifestMappings[m.NetworkName] = m
	}
}

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

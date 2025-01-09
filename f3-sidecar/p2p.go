package main

import (
	"context"
	"fmt"

	"github.com/libp2p/go-libp2p"
	dht "github.com/libp2p/go-libp2p-kad-dht"
	pubsub "github.com/libp2p/go-libp2p-pubsub"
	"github.com/libp2p/go-libp2p/core/host"
	"github.com/libp2p/go-libp2p/core/protocol"
)

const ListenAddr = "/ip4/127.0.0.1/tcp/0"

type P2PHost struct {
	Host      host.Host
	DHT       *dht.IpfsDHT
	BackupDHT *dht.IpfsDHT
	PubSub    *pubsub.PubSub
}

func createP2PHost(ctx context.Context, networkName string) (*P2PHost, error) {
	host, err := libp2p.New(libp2p.ListenAddrStrings(ListenAddr))
	if err != nil {
		return nil, err
	}

	dthOpts := []dht.Option{
		dht.Mode(dht.ModeAutoServer),
		dht.ProtocolPrefix(protocol.ID(fmt.Sprintf("/fil/kad/%s", networkName))),
		dht.DisableProviders(),
		dht.DisableValues(),
	}
	hostDHT, err := dht.New(ctx, host, dthOpts...)
	if err != nil {
		return nil, err
	}

	backupDthOpts := []dht.Option{
		dht.Mode(dht.ModeAutoServer),
		dht.ProtocolPrefix(protocol.ID(fmt.Sprintf("/fil/kad/f3-sidecar/%s", networkName))),
		dht.DisableProviders(),
		dht.DisableValues(),
	}
	backupHostDHT, err := dht.New(ctx, host, backupDthOpts...)
	if err != nil {
		return nil, err
	}

	ps, err := pubsub.NewGossipSub(ctx, host,
		pubsub.WithPeerExchange(true),
		pubsub.WithFloodPublish(true),
		pubsub.WithPeerScore(PubsubPeerScoreParams, PubsubPeerScoreThresholds))
	if err != nil {
		return nil, err
	}

	return &P2PHost{host, hostDHT, backupHostDHT, ps}, nil
}

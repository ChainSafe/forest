package main

import (
	"context"
	"fmt"

	"github.com/libp2p/go-libp2p"
	dht "github.com/libp2p/go-libp2p-kad-dht"
	pubsub "github.com/libp2p/go-libp2p-pubsub"
	pubsub_pb "github.com/libp2p/go-libp2p-pubsub/pb"
	"github.com/libp2p/go-libp2p/core/host"
	"github.com/libp2p/go-libp2p/core/protocol"
	"golang.org/x/crypto/blake2b"
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
		pubsub.WithMessageIdFn(hashMsgId),
		// Bump the validation queue to accommodate the increase in gossipsub message
		// exchange rate as a result of f3. The size of 4096 should offer enough headroom
		// for slower F3 validation while avoiding: 1) avoid excessive memory usage, 2)
		// dropped consensus related messages and 3) cascading effect among other topics
		// since this config isn't topic-specific.
		pubsub.WithValidateQueueSize(4096),
		pubsub.WithPeerScore(PubsubPeerScoreParams, PubsubPeerScoreThresholds))
	if err != nil {
		return nil, err
	}

	return &P2PHost{host, hostDHT, backupHostDHT, ps}, nil
}

func hashMsgId(m *pubsub_pb.Message) string {
	hash := blake2b.Sum256(m.Data)
	return string(hash[:])
}

package main

import (
	"context"

	logging "github.com/ipfs/go-log/v2"
	"github.com/libp2p/go-libp2p"
	dht "github.com/libp2p/go-libp2p-kad-dht"
	"github.com/libp2p/go-libp2p/core/host"
	"github.com/libp2p/go-libp2p/core/peer"
	ma "github.com/multiformats/go-multiaddr"
)

func init() {
	setGoDebugEnv()
	err := logging.SetLogLevel("dht", "debug")
	checkError(err)
	GoKadNodeImpl = &kadImpl{ctx: context.Background()}
}

type kadImpl struct {
	ctx  context.Context
	node *kadNode
}

type kadNode struct {
	host    host.Host
	hostDHT *dht.IpfsDHT
}

func (impl *kadImpl) run() {
	host, err := libp2p.New(libp2p.ListenAddrStrings(ListenAddr))
	checkError(err)

	dthOpts := []dht.Option{
		dht.Mode(dht.ModeServer),
		dht.ProtocolPrefix("/kadtest"),
		dht.DisableProviders(),
		dht.DisableValues(),
	}
	hostDHT, err := dht.New(impl.ctx, host, dthOpts...)
	checkError(err)

	impl.node = &kadNode{host, hostDHT}
}

func (impl *kadImpl) connect(multiaddr *string) {
	targetAddr, err := ma.NewMultiaddr(*multiaddr)
	checkError(err)

	target, err := peer.AddrInfoFromP2pAddr(targetAddr)
	checkError(err)

	err = impl.node.host.Connect(impl.ctx, *target)
	checkError(err)

}

func (impl *kadImpl) get_n_connected() uint {
	return uint(impl.node.host.Peerstore().Peers().Len())
}

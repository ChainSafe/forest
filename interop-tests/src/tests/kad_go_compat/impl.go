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

const (
	ListenAddr = "/ip4/127.0.0.1/tcp/0"
)

func init() {
	err := logging.SetLogLevel("dht", "debug")
	checkError(err)
	GoKadNodeImpl = &Impl{ctx: context.Background()}
}

type Impl struct {
	ctx  context.Context
	node *Node
}

type Node struct {
	host    host.Host
	hostDHT *dht.IpfsDHT
}

func (impl *Impl) run() {
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

	impl.node = &Node{host, hostDHT}
}

func (impl *Impl) connect(multiaddr string) {
	targetAddr, err := ma.NewMultiaddr(multiaddr)
	checkError(err)

	target, err := peer.AddrInfoFromP2pAddr(targetAddr)
	checkError(err)

	if err := impl.node.host.Connect(impl.ctx, *target); err != nil {
		panic(err)
	}
}

func (impl *Impl) get_n_connected(req EmptyReq) uint {
	return uint(impl.node.host.Peerstore().Peers().Len())
}

func checkError(err error) {
	if err != nil {
		panic(err)
	}
}

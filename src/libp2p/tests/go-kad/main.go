package main

import (
	"context"
	"flag"
	"fmt"
	"time"

	logging "github.com/ipfs/go-log/v2"
	"github.com/libp2p/go-libp2p"
	dht "github.com/libp2p/go-libp2p-kad-dht"
	"github.com/libp2p/go-libp2p/core/peer"
	ma "github.com/multiformats/go-multiaddr"
)

const (
	ListenAddr = "/ip4/127.0.0.1/tcp/0"
)

func main() {
	err := logging.SetLogLevel("dht", "debug")
	checkError(err)

	var targetMultiaddr string

	flag.StringVar(&targetMultiaddr, "addr", "", "peer multiaddr")

	flag.Parse()

	println("targetMultiaddr:", targetMultiaddr)

	ctx := context.Background()
	targetAddr, err := ma.NewMultiaddr(targetMultiaddr)
	checkError(err)

	target, err := peer.AddrInfoFromP2pAddr(targetAddr)
	checkError(err)

	host, err := libp2p.New(libp2p.ListenAddrStrings(ListenAddr))
	checkError(err)

	dthOpts := []dht.Option{
		dht.Mode(dht.ModeServer),
		dht.ProtocolPrefix("/kadtest"),
		dht.DisableProviders(),
		dht.DisableValues(),
	}
	hostDHT, err := dht.New(ctx, host, dthOpts...)
	checkError(err)

	if err := host.Connect(ctx, *target); err != nil {
		panic(err)
	}
	if err := hostDHT.Bootstrap(ctx); err != nil {
		panic(err)
	}
	fmt.Printf("peer id: %s\n", host.ID())
	for host.Peerstore().Peers().Len() <= 2 {
		fmt.Printf("peer count: %d\n", host.Peerstore().Peers().Len())
		time.Sleep(1 * time.Second)
	}
	fmt.Printf("Success. More peers(%d) are connected to via kademlia\n", host.Peerstore().Peers().Len())
}

func checkError(err error) {
	if err != nil {
		panic(err)
	}
}

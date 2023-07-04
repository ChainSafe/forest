package main

import (
	"context"
	"flag"
	"time"

	"github.com/ipfs/boxo/bitswap"
	bsnet "github.com/ipfs/boxo/bitswap/network"
	"github.com/ipfs/boxo/blockstore"
	"github.com/ipfs/go-cid"
	ds "github.com/ipfs/go-datastore"
	util "github.com/ipfs/go-ipfs-util"
	"github.com/libp2p/go-libp2p"
	"github.com/libp2p/go-libp2p/core/peer"
	ma "github.com/multiformats/go-multiaddr"
)

const (
	ListenAddr = "/ip4/127.0.0.1/tcp/0"
)

func main() {
	var targetMultiaddr string
	var expectedCid string

	flag.StringVar(&targetMultiaddr, "addr", "", "peer multiaddr")
	flag.StringVar(&expectedCid, "cid", "", "cid")

	flag.Parse()

	println("targetMultiaddr:", targetMultiaddr)
	println("expectedCid:", expectedCid)

	ctx := context.Background()
	targetAddr, err := ma.NewMultiaddr(targetMultiaddr)
	if err != nil {
		panic(err)
	}
	target, err := peer.AddrInfoFromP2pAddr(targetAddr)
	if err != nil {
		panic(err)
	}
	host, _ := libp2p.New(libp2p.ListenAddrStrings(ListenAddr))
	ttl, _ := time.ParseDuration("24h")
	host.Peerstore().AddAddrs(target.ID, target.Addrs, ttl)
	router := NewRouter(*target)
	network := bsnet.NewFromIpfsHost(host, router, bsnet.Prefix("/test"))
	bstore := blockstore.NewBlockstore(ds.NewMapDatastore())
	exchange := bitswap.New(ctx, network, bstore)
	id := cid.NewCidV0(util.Hash([]byte(expectedCid)))
	r, err := exchange.GetBlock(ctx, id)
	if err != nil {
		panic(err)
	}
	print(r)
}

type NaiveRouter struct {
	p peer.AddrInfo
}

func NewRouter(p peer.AddrInfo) NaiveRouter {
	return NaiveRouter{p}
}

func (r NaiveRouter) Provide(ctx context.Context, k cid.Cid, b bool) error { return nil }

func (r NaiveRouter) FindProvidersAsync(ctx context.Context, k cid.Cid, max int) <-chan peer.AddrInfo {
	ch := make(chan peer.AddrInfo)
	go func() {
		ch <- r.p
	}()
	return ch
}

package main

import (
	"context"
	"time"

	"github.com/ipfs/boxo/bitswap"
	bsnet "github.com/ipfs/boxo/bitswap/network"
	"github.com/ipfs/boxo/blockstore"
	"github.com/ipfs/go-cid"
	ds "github.com/ipfs/go-datastore"
	util "github.com/ipfs/go-ipfs-util"
	logging "github.com/ipfs/go-log/v2"
	"github.com/libp2p/go-libp2p"
	"github.com/libp2p/go-libp2p/core/host"
	"github.com/libp2p/go-libp2p/core/peer"
	ma "github.com/multiformats/go-multiaddr"
)

func init() {
	logging.SetDebugLogging()
	GoBitswapNodeImpl = &bitswapImpl{ctx: context.Background()}
}

type bitswapImpl struct {
	ctx  context.Context
	node *bitswapNode
}

type bitswapNode struct {
	host     host.Host
	exchange *bitswap.Bitswap
}

func (impl *bitswapImpl) run() {
	host, err := libp2p.New(libp2p.ListenAddrStrings(ListenAddr))
	checkError(err)

	impl.node = &bitswapNode{host: host}
}

func (impl *bitswapImpl) connect(multiaddr string) {
	targetAddr, err := ma.NewMultiaddr(multiaddr)
	checkError(err)

	target, err := peer.AddrInfoFromP2pAddr(targetAddr)
	checkError(err)

	ttl, _ := time.ParseDuration("24h")
	impl.node.host.Peerstore().AddAddrs(target.ID, target.Addrs, ttl)

	router := NewRouter(*target)
	network := bsnet.NewFromIpfsHost(impl.node.host, router, bsnet.Prefix("/test"))
	bstore := blockstore.NewBlockstore(ds.NewMapDatastore())
	exchange := bitswap.New(impl.ctx, network, bstore)
	impl.node.exchange = exchange
}

func (impl *bitswapImpl) get_block(expectedCid string) bool {
	id := cid.NewCidV0(util.Hash([]byte(expectedCid)))
	_, err := impl.node.exchange.GetBlock(impl.ctx, id)
	checkError(err)
	return err == nil
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

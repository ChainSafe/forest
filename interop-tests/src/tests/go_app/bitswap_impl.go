package main

import (
	"context"

	"github.com/ipfs/boxo/bitswap"
	"github.com/ipfs/boxo/bitswap/network/bsnet"
	"github.com/ipfs/boxo/blockstore"
	"github.com/ipfs/go-cid"
	ds "github.com/ipfs/go-datastore"
	logging "github.com/ipfs/go-log/v2"
	"github.com/libp2p/go-libp2p"
	"github.com/libp2p/go-libp2p/core/host"
	"github.com/libp2p/go-libp2p/core/peer"
	"github.com/libp2p/go-libp2p/core/peerstore"
	ma "github.com/multiformats/go-multiaddr"
)

var logger = logging.Logger("bitswap/test")

func init() {
	setGoDebugEnv()
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

func (impl *bitswapImpl) connect(multiaddr *string) {
	targetAddr, err := ma.NewMultiaddr(*multiaddr)
	checkError(err)

	target, err := peer.AddrInfoFromP2pAddr(targetAddr)
	checkError(err)

	impl.node.host.Peerstore().AddAddrs(target.ID, target.Addrs, peerstore.PermanentAddrTTL)

	router := NewRouter(*target)
	network := bsnet.NewFromIpfsHost(impl.node.host, bsnet.Prefix("/test"))
	bstore := blockstore.NewBlockstore(ds.NewMapDatastore())
	exchange := bitswap.New(impl.ctx, network, router, bstore)
	impl.node.exchange = exchange
}

func (impl *bitswapImpl) get_block(cidStr *string) bool {
	id, err := cid.Parse(*cidStr)
	checkError(err)
	b, err := impl.node.exchange.GetBlock(impl.ctx, id)
	checkError(err)
	data := string(b.RawData())
	logger.Infof("got block, cid=%s, data=%s", id, data)
	return err == nil
}

type NaiveRouter struct {
	p peer.AddrInfo
}

func NewRouter(p peer.AddrInfo) NaiveRouter {
	return NaiveRouter{p}
}

func (r NaiveRouter) FindProvidersAsync(ctx context.Context, k cid.Cid, max int) <-chan peer.AddrInfo {
	ch := make(chan peer.AddrInfo)
	go func() {
		ch <- r.p
	}()
	return ch
}

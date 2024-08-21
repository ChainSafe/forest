package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"os"
	"time"

	"github.com/filecoin-project/go-f3"
	"github.com/filecoin-project/go-f3/blssig"
	"github.com/filecoin-project/go-f3/gpbft"
	"github.com/filecoin-project/go-f3/manifest"
	"github.com/filecoin-project/go-jsonrpc"
	leveldb "github.com/ipfs/go-ds-leveldb"
	logging "github.com/ipfs/go-log/v2"
)

var logger = logging.Logger("f3-sidecar")

func main() {
	logging.SetAllLoggers(logging.LevelError)
	if err := logging.SetLogLevel("f3-sidecar", "debug"); err != nil {
		panic(err)
	}
	if err := logging.SetLogLevel("f3", "debug"); err != nil {
		panic(err)
	}

	var rpcEndpoint string
	flag.StringVar(&rpcEndpoint, "rpc", "http://127.0.0.1:2345/rpc/v1", "forest RPC endpoint")
	flag.Parse()

	ctx := context.Background()

	api := FilecoinApi{}
	closer, err := jsonrpc.NewClient(context.Background(), rpcEndpoint, "Filecoin", &api, nil)
	if err != nil {
		panic(err)
	}
	defer closer()
	network, err := api.StateNetworkName(ctx)
	if err != nil {
		panic(err)
	}
	listenAddrs, err := api.NetAddrsListen(ctx)
	if err != nil {
		panic(err)
	}

	p2p, err := createP2PHost(ctx, network)
	if err != nil {
		panic(err)
	}
	ec, err := NewForestEC(rpcEndpoint)
	if err != nil {
		panic(err)
	}
	if _, err = ec.f3api.ProtectPeer(ctx, p2p.Host.ID()); err != nil {
		panic(err)
	}
	err = p2p.Host.Connect(ctx, listenAddrs)
	if err != nil {
		panic(err)
	}
	tmpdir, err := os.MkdirTemp("", "f3-*")
	if err != nil {
		panic(err)
	}
	ds, err := leveldb.NewDatastore(tmpdir, nil)
	if err != nil {
		panic(err)
	}
	verif := blssig.VerifierWithKeyOnG1()
	m := manifest.LocalDevnetManifest()
	m.NetworkName = gpbft.NetworkName(network)
	m.EC.Period = 30 * time.Second
	head, err := ec.GetHead(ctx)
	if err != nil {
		panic(err)
	}
	m.BootstrapEpoch = head.Epoch() - 100
	m.EC.Finality = 900
	m.CommitteeLookback = 5
	// m.Pause = true

	f3Module, err := f3.New(ctx, manifest.NewStaticManifestProvider(m), ds,
		p2p.Host, p2p.PubSub, verif, &ec)
	if err != nil {
		panic(err)
	}
	if err := f3Module.Start(ctx); err != nil {
		panic(err)
	}
	nMessageToSign := 0
	for {
		msgToSign := <-f3Module.MessagesToSign()
		nMessageToSign += 1
		fmt.Printf("Message to sign: %d\n", nMessageToSign)
		miners, err := ec.f3api.GetParticipatedMinerIDs(ctx)
		if err != nil {
			continue
		}
		for _, miner := range miners {
			signatureBuilder, err := msgToSign.PrepareSigningInputs(gpbft.ActorID(miner))
			if err != nil {
				if errors.Is(err, gpbft.ErrNoPower) {
					// we don't have any power in F3, continue
					logger.Warnf("no power to participate in F3: %+v", err)
				} else {
					logger.Warnf("preparing signing inputs: %+v", err)
				}
				continue
			}
			payloadSig, vrfSig, err := signatureBuilder.Sign(ctx, &ec)
			if err != nil {
				logger.Warnf("signing message: %+v", err)
			}
			logger.Debugf("miner with id %d is sending message in F3", miner)
			f3Module.Broadcast(ctx, signatureBuilder, payloadSig, vrfSig)
		}
	}
}

package main

import (
	"context"
	"errors"
	"net"
	"net/http"
	"time"

	"github.com/filecoin-project/go-f3"
	"github.com/filecoin-project/go-f3/blssig"
	"github.com/filecoin-project/go-f3/gpbft"
	"github.com/filecoin-project/go-f3/manifest"
	"github.com/filecoin-project/go-jsonrpc"
	"github.com/ipfs/go-datastore"
	"github.com/ipfs/go-datastore/namespace"
	leveldb "github.com/ipfs/go-ds-leveldb"
	"github.com/libp2p/go-libp2p/core/peer"
)

func run(ctx context.Context, rpcEndpoint string, f3RpcSocketAddress string, finality int64, db string, manifestServer string) error {
	api := FilecoinApi{}
	closer, err := jsonrpc.NewClient(context.Background(), rpcEndpoint, "Filecoin", &api, nil)
	if err != nil {
		return err
	}
	defer closer()
	network, err := api.StateNetworkName(ctx)
	if err != nil {
		return err
	}
	listenAddrs, err := api.NetAddrsListen(ctx)
	if err != nil {
		return err
	}

	p2p, err := createP2PHost(ctx, network)
	if err != nil {
		return err
	}
	ec, err := NewForestEC(rpcEndpoint)
	if err != nil {
		return err
	}
	defer ec.Close()
	if _, err = ec.f3api.ProtectPeer(ctx, p2p.Host.ID()); err != nil {
		return err
	}
	err = p2p.Host.Connect(ctx, listenAddrs)
	if err != nil {
		return err
	}
	ds, err := leveldb.NewDatastore(db, nil)
	if err != nil {
		return err
	}
	verif := blssig.VerifierWithKeyOnG1()
	m := manifest.LocalDevnetManifest()
	m.NetworkName = gpbft.NetworkName(network)
	versionInfo, err := api.Version(ctx)
	if err != nil {
		return err
	}
	m.EC.Period = time.Duration(versionInfo.BlockDelay) * time.Second
	head, err := ec.GetHead(ctx)
	if err != nil {
		return err
	}
	m.EC.Finality = finality
	m.BootstrapEpoch = max(m.EC.Finality+1, head.Epoch()-m.EC.Finality+1)
	m.CommitteeLookback = 5
	// m.Pause = true

	var manifestProvider manifest.ManifestProvider
	switch manifestServerID, err := peer.Decode(manifestServer); {
	case err != nil:
		logger.Info("Using static manifest provider")
		manifestProvider = manifest.NewStaticManifestProvider(m)
	default:
		logger.Infof("Using dynamic manifest provider at %s", manifestServerID)
		manifestDS := namespace.Wrap(ds, datastore.NewKey("/f3-dynamic-manifest"))
		manifestProvider = manifest.NewDynamicManifestProvider(m, manifestDS, p2p.PubSub, manifestServerID)
	}

	f3Module, err := f3.New(ctx, manifestProvider, ds,
		p2p.Host, p2p.PubSub, verif, &ec)
	if err != nil {
		return err
	}
	if err := f3Module.Start(ctx); err != nil {
		return err
	}

	rpcServer := jsonrpc.NewServer()
	serverHandler := &F3ServerHandler{f3Module}
	rpcServer.Register("Filecoin", serverHandler)
	srv := &http.Server{
		Handler: rpcServer,
	}
	listener, err := net.Listen("tcp", f3RpcSocketAddress)
	if err != nil {
		return err
	}
	go func() {
		if err := srv.Serve(listener); err != nil {
			panic(err)
		}
	}()

	for {
		msgToSign := <-f3Module.MessagesToSign()
		miners, err := ec.f3api.GetParticipatingMinerIDs(ctx)
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

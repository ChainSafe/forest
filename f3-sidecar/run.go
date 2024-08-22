package main

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/filecoin-project/go-f3"
	"github.com/filecoin-project/go-f3/blssig"
	"github.com/filecoin-project/go-f3/gpbft"
	"github.com/filecoin-project/go-f3/manifest"
	"github.com/filecoin-project/go-jsonrpc"
	leveldb "github.com/ipfs/go-ds-leveldb"
)

func run(ctx context.Context, rpcEndpoint string, finality int64, db string) error {
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

	f3Module, err := f3.New(ctx, manifest.NewStaticManifestProvider(m), ds,
		p2p.Host, p2p.PubSub, verif, &ec)
	if err != nil {
		return err
	}
	if err := f3Module.Start(ctx); err != nil {
		return err
	}

	// Goroutine for debugging
	go func() {
		for {
			time.Sleep(10 * time.Second)
			cert, err := f3Module.GetLatestCert(ctx)
			if err != nil {
				logger.Warnf("GetLatestCert %s", err)
				continue
			}
			logger.Infof("Cert: instance: %d, ec chain base: %d head: %d", cert.GPBFTInstance, cert.ECChain.Base().Epoch, cert.ECChain.Head().Epoch)
		}
	}()

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
			miner = 1000
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

package main

import (
	"context"
	"errors"
	"fmt"
	"net"
	"net/http"
	"path/filepath"
	"time"

	"github.com/filecoin-project/go-f3"
	"github.com/filecoin-project/go-f3/blssig"
	"github.com/filecoin-project/go-f3/gpbft"
	"github.com/filecoin-project/go-f3/manifest"
	"github.com/filecoin-project/go-jsonrpc"
	"github.com/ipfs/go-cid"
	"github.com/ipfs/go-datastore"
	"github.com/ipfs/go-datastore/namespace"
	leveldb "github.com/ipfs/go-ds-leveldb"
	"github.com/libp2p/go-libp2p/core/peer"
)

func run(ctx context.Context, rpcEndpoint string, jwt string, f3RpcEndpoint string, initialPowerTable string, bootstrapEpoch int64, finality int64, f3Root string, manifestServer string) error {
	api := FilecoinApi{}
	isJwtProvided := len(jwt) > 0
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
	ec, err := NewForestEC(rpcEndpoint, jwt)
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
	ds, err := leveldb.NewDatastore(filepath.Join(f3Root, "db"), nil)
	if err != nil {
		return err
	}
	verif := blssig.VerifierWithKeyOnG1()
	m := manifest.LocalDevnetManifest()
	switch initialPowerTable, err := cid.Parse(initialPowerTable); {
	case err == nil && initialPowerTable != cid.Undef:
		logger.Infof("InitialPowerTable is %s", initialPowerTable)
		m.InitialPowerTable = initialPowerTable
	default:
		logger.Warn("InitialPowerTable is undefined")
		m.InitialPowerTable = cid.Undef
	}
	m.NetworkName = gpbft.NetworkName(network)
	versionInfo, err := api.Version(ctx)
	if err != nil {
		return err
	}

	blockDelay := time.Duration(versionInfo.BlockDelay) * time.Second
	m.EC.Period = blockDelay
	m.EC.HeadLookback = 4
	m.CatchUpAlignment = blockDelay / 2
	m.CertificateExchange.MinimumPollInterval = blockDelay
	m.CertificateExchange.MaximumPollInterval = 4 * blockDelay

	head, err := ec.GetHead(ctx)
	if err != nil {
		return err
	}
	m.EC.Finality = finality
	if bootstrapEpoch < 0 {
		// This is temporary logic to make the dummy bootstrap epoch work locally.
		// It should be removed once bootstrapEpochs are determinted.
		m.BootstrapEpoch = max(m.EC.Finality+1, head.Epoch()-m.EC.Finality+1)
	} else {
		m.BootstrapEpoch = bootstrapEpoch
	}
	m.CommitteeLookback = manifest.DefaultCommitteeLookback

	var manifestProvider manifest.ManifestProvider
	switch manifestServerID, err := peer.Decode(manifestServer); {
	case err != nil:
		logger.Info("Using static manifest provider")
		if manifestProvider, err = manifest.NewStaticManifestProvider(m); err != nil {
			return err
		}
	default:
		logger.Infof("Using dynamic manifest provider at %s", manifestServerID)
		manifestDS := namespace.Wrap(ds, datastore.NewKey("/f3-dynamic-manifest"))
		primaryNetworkName := m.NetworkName
		filter := func(m *manifest.Manifest) error {
			if m.EC.Finalize {
				return fmt.Errorf("refusing dynamic manifest that finalizes tipsets")
			}
			if m.NetworkName == primaryNetworkName {
				return fmt.Errorf(
					"refusing dynamic manifest with network name %q that clashes with initial manifest",
					primaryNetworkName,
				)
			}
			return nil
		}
		if manifestProvider, err = manifest.NewDynamicManifestProvider(
			p2p.PubSub,
			manifestServerID,
			manifest.DynamicManifestProviderWithInitialManifest(m),
			manifest.DynamicManifestProviderWithDatastore(manifestDS),
			manifest.DynamicManifestProviderWithFilter(filter)); err != nil {
			return err
		}
	}

	f3Module, err := f3.New(ctx, manifestProvider, ds,
		p2p.Host, p2p.PubSub, verif, &ec, f3Root)
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
	listener, err := net.Listen("tcp", f3RpcEndpoint)
	if err != nil {
		return err
	}
	go func() {
		if err := srv.Serve(listener); err != nil {
			panic(err)
		}
	}()

	var lastMsgToSignTimestamp time.Time
	var lastMsgToSign *gpbft.MessageBuilder
	lastMsgSigningMiners := make(map[uint64]struct{})

	// Send the last gpbft message for each new participant,
	// see <https://github.com/filecoin-project/lotus/pull/12577>
	if isJwtProvided {
		go func() {
			for {
				// Send only when no messages are received in the last 10s.
				// This is to avoid a deadlock situation where everyone is waiting
				// for the next round to participate, but we'll never get there
				// because not enough participants acted in the current round.
				if lastMsgToSign != nil && lastMsgToSignTimestamp.Add(10*time.Second).Before(time.Now()) {
					if miners, err := ec.f3api.GetParticipatingMinerIDs(ctx); err == nil {
						for _, miner := range miners {
							if _, ok := lastMsgSigningMiners[miner]; ok {
								continue
							} else if err := participate(ctx, f3Module, &ec, lastMsgToSign, miner); err != nil {
								logger.Warn(err)
							} else {
								lastMsgSigningMiners[miner] = struct{}{}
							}
						}
					}
				}

				time.Sleep(1 * time.Second)
			}
		}()
	}

	for {
		msgToSign := <-f3Module.MessagesToSign()
		lastMsgToSignTimestamp = time.Now()
		lastMsgToSign = msgToSign
		miners, err := ec.f3api.GetParticipatingMinerIDs(ctx)
		if err != nil {
			continue
		}
		// Clear the map
		clear(lastMsgSigningMiners)
		if !isJwtProvided && len(miners) > 0 {
			logger.Warn("Unable to sign messages, jwt for Forest RPC endpoint is not provided.")
		}
		if isJwtProvided && msgToSign != nil {
			for _, miner := range miners {
				if err := participate(ctx, f3Module, &ec, msgToSign, miner); err != nil {
					logger.Warn(err)
				} else {
					lastMsgSigningMiners[miner] = struct{}{}
				}
			}
		}
	}
}

func participate(ctx context.Context, f3Module *f3.F3, signer gpbft.Signer, msgToSign *gpbft.MessageBuilder, miner uint64) error {
	signatureBuilder, err := msgToSign.PrepareSigningInputs(gpbft.ActorID(miner))
	if err != nil {
		if errors.Is(err, gpbft.ErrNoPower) {
			// we don't have any power in F3, continue
			return fmt.Errorf("no power to participate in F3: %+v", err)
		} else {
			return fmt.Errorf("preparing signing inputs: %+v", err)
		}
	}
	payloadSig, vrfSig, err := signatureBuilder.Sign(ctx, signer)
	if err != nil {
		logger.Warnf("signing message: %+v", err)
	}
	logger.Debugf("miner with id %d is sending message in F3", miner)
	f3Module.Broadcast(ctx, signatureBuilder, payloadSig, vrfSig)
	return nil
}

package main

import (
	"time"

	pubsub "github.com/libp2p/go-libp2p-pubsub"
	"github.com/libp2p/go-libp2p/core/peer"
)

func init() {
	// "borrowed" from lotus node/modules/lp2p/pubsub.go
	// configure larger overlay parameters
	pubsub.GossipSubD = 8
	pubsub.GossipSubDscore = 6
	pubsub.GossipSubDout = 3
	pubsub.GossipSubDlo = 6
	pubsub.GossipSubDhi = 12
	pubsub.GossipSubDlazy = 12
	pubsub.GossipSubDirectConnectInitialDelay = 30 * time.Second
	pubsub.GossipSubIWantFollowupTime = 5 * time.Second
	pubsub.GossipSubHistoryLength = 10
	pubsub.GossipSubGossipFactor = 0.1
}

// Borrowed from lotus
var PubsubPeerScoreParams = &pubsub.PeerScoreParams{
	AppSpecificScore:  func(p peer.ID) float64 { return 0 },
	AppSpecificWeight: 1,

	// This sets the IP colocation threshold to 5 peers before we apply penalties
	IPColocationFactorThreshold: 5,
	IPColocationFactorWeight:    -100,
	IPColocationFactorWhitelist: nil,

	// P7: behavioural penalties, decay after 1hr
	BehaviourPenaltyThreshold: 6,
	BehaviourPenaltyWeight:    -10,
	BehaviourPenaltyDecay:     pubsub.ScoreParameterDecay(time.Hour),

	DecayInterval: pubsub.DefaultDecayInterval,
	DecayToZero:   pubsub.DefaultDecayToZero,

	// this retains non-positive scores for 6 hours
	RetainScore: 6 * time.Hour,

	// topic parameters
	Topics: make(map[string]*pubsub.TopicScoreParams),
}

var PubsubPeerScoreThresholds = &pubsub.PeerScoreThresholds{
	GossipThreshold:             GossipScoreThreshold,
	PublishThreshold:            PublishScoreThreshold,
	GraylistThreshold:           GraylistScoreThreshold,
	AcceptPXThreshold:           AcceptPXScoreThreshold,
	OpportunisticGraftThreshold: OpportunisticGraftScoreThreshold,
}

// Borrowed from lotus
const (
	GossipScoreThreshold             = -500
	PublishScoreThreshold            = -1000
	GraylistScoreThreshold           = -2500
	AcceptPXScoreThreshold           = 1000
	OpportunisticGraftScoreThreshold = 3.5
)

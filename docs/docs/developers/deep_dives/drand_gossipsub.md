---
title: Drand over Gossipsub
sidebar_position: 2
---

Forest listens to the `drand` relays over `gossipsub` and caches the valid rounds it receives, so block validation stops fetching them over HTTP. HTTP remains the fallback for a cache miss.

Five views, from the outside in: who Forest talks to, what runs inside it, how the network service is put together, when a verdict is decided, and what happens to one gossiped round.

## The overall view

The gossipsub mesh publishes rounds and Forest forwards the ones it verifies, so Forest is a relay to its peers as well as a consumer.

```mermaid
flowchart TB
  subgraph mesh["Drand GossipSub Mesh<br><i>They publish/broadcast each quicknet round on a gossipsub topic</i>"]
    direction LR
    relays["Drand Relays"]
    lotus["Lotus Peers"]
    peers["Forest Peers"]
  end
  http["Drand HTTP API<br><i>Provides rounds on request, the fallback for rounds the cache doesn't have</i>"]
  forest["Forest<br><i>Holds a cache of Drand rounds and answers round lookups from it</i>"]

  mesh -- "Publishes rounds" --> forest
  forest -- "Forwards verified rounds" --> mesh
  forest -- "Fetches a round on a cache miss" --> http

  style peers stroke-dasharray: 5 5
```

## Zoom-in on Forest

Two parts of Forest take part. The network service talks to the mesh and reports every verdict. The beacon's gossip listener verifies and caches the rounds the service hands it, and sends the verdicts back; the beacon also talks to the HTTP API. Block validation reads the same cache.

```mermaid
flowchart LR
  mesh["Drand GossipSub Mesh<br><i>Drand relays, Lotus peers and eventually other Forest nodes</i>"]
  http["Drand HTTP API<br><i>The fallback for rounds the cache doesn't have</i>"]

  subgraph forest["Forest<br><i>Holds a cache of Drand rounds and answers round lookups from it</i>"]
    direction TB
    libp2p["Libp2p Service<br><i>Subscribes to the drand topic. Receives rounds from the mesh, reports a verdict for each, forwards the accepted ones</i>"]
    beacon["Beacon<br><i>Its gossip listener verifies rounds against the Drand public key and caches the verified ones. Fetches over HTTP on a cache miss</i>"]
    libp2p -- "Verify & cache this round" --> beacon
    beacon -- "Verdict" --> libp2p
  end

  mesh -- "Publishes rounds" --> libp2p
  libp2p -- "Forwards accepted rounds" --> mesh
  beacon -- "Fetches a round on a cache miss" --> http
```

## Components of the `libp2p` service

The service touches the listener at two points: a bounded channel in, fed by the gossip handler, and a channel out, carrying the verdict. Everything `drand`-specific lives in `beacon::gossipsub`.

```mermaid
flowchart LR
  mesh["Drand GossipSub Mesh [System]<br><i>Drand relays, Lotus peers and eventually other Forest nodes</i>"]
  beacon["Beacon [Component]<br><i>Verifies rounds against the Drand public key and caches the verified ones. Fetches over HTTP on a cache miss</i>"]

  subgraph libp2p["Libp2p Service [Component]<br><i>Subscribes to the drand topic. Receives rounds from the mesh, reports a verdict for each, forwards the accepted ones</i>"]
    direction TB
    gs["Gossipsub Behaviour<br><i>Keeps the mesh and delivers each message once. Holds a message until a verdict is reported, then forwards or drops it. Scores peers</i>"]
    handler["Gossip event handler<br><i>Dispatches each message by topic. The Drand topic goes to the listener's queue</i>"]
    gs -- "Gossipsub message goes to the event handler" --> handler
  end

  listener["Drand Gossip Listener [Component, beacon::gossipsub]<br><i>Owns the topic. Runs the cheap checks, verifies against the beacon, records the verdict metric and sends the verdict back</i>"]

  mesh -- "Publishes rounds" --> gs
  handler -- "Round, over a bounded channel" --> listener
  listener -- "Verdict, reported by the service loop" --> gs
  listener -- "Verifies & caches this entry" --> beacon
```

## When the verdict is decided

The `gossipsub` behaviour holds every message until a verdict is reported. A `drand` round is queued for the listener, which runs the cheap checks and one BLS verification on its own task and sends the verdict back; a later `select!` iteration reports it. When the queue is full the handler waits for the listener.

```mermaid
flowchart TB
  subgraph gs["Gossipsub Behaviour"]
    direction LR
    rx["Message arrives from a peer"]
    hold["Hold it until a verdict is reported"]
    report["report_message_validation_result"]
    rx --> hold
  end

  subgraph loop["Service Event Loop [select!]"]
    direction LR
    event["Gossip message event<br>[handle_gossip_event]"]
    topic{"Which topic"}
    decode["Decode & emit to chain sync or mpool"]
    offer["Queue it for the listener<br>[emit_event]"]
    verdict["Verdict arrives<br>[drand_verdicts arm]"]
    event --> topic
    topic -- "/fil/blocks or /fil/msgs" --> decode
    topic -- "/drand/pubsub/..." --> offer
  end

  subgraph drand["Drand Gossip Listener [beacon::gossipsub task]"]
    direction LR
    run["Next round<br>[run]"]
    checks["Cheap checks"]
    verify["Verify against the beacon"]
    decided["Verdict sent back"]
    run --> checks
    checks --> verify
    checks -- "if an early check fails" --> decided
    verify --> decided
  end

  hold --> event
  offer -- "bounded channel" --> run
  decided -- "verdict channel" --> verdict
  verdict --> report
  decode -- "Some(Verdict)" --> report

  style report fill:#fff3bf,stroke:#f0a500
```

## Verdicts

| Verdict | Forwarded | Sender penalised | Used for                                                                        |
| ------- | --------- | ---------------- | ------------------------------------------------------------------------------- |
| Accept  | yes       | no               | a round that verifies against the chain's public key and wasn't cached yet      |
| Ignore  | no        | no               | anything an honest peer may send that we can't use: off-epoch, stale, duplicate |
| Reject  | no        | yes              | what no honest peer sends: undecodable payloads, bad or forged signatures       |

Only unchained networks (`quicknet`) are gossiped. As in Lotus there is no time window: a round only has to be newer than the last one accepted, so a slow local clock never loses a round, and a forged one is rejected by its signature.

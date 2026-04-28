// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Ephemeral libp2p peer for ad-hoc protocol probing. Dials a single peer,
//! optionally performs a Hello handshake, then sends one or more Filecoin
//! request/response protocol messages over that connection.

use std::io;
use std::num::NonZeroUsize;
use std::str::FromStr;
use std::sync::LazyLock;
use std::time::Duration;

use ahash::{HashSet, HashSetExt as _};
use anyhow::{Context as _, anyhow, bail};
use async_trait::async_trait;
use cid::Cid;
use clap::{Parser, Subcommand};
use futures::io::{AsyncRead, AsyncWrite};
use futures::stream::StreamExt as _;
use libp2p::{
    Multiaddr, PeerId, Swarm, SwarmBuilder, identify,
    multiaddr::Protocol,
    noise,
    request_response::{self, OutboundRequestId, ProtocolSupport},
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux,
};
use nonzero_ext::nonzero;
use num_bigint::BigInt as RawBigInt;
use nunny::Vec as NonEmpty;

use crate::libp2p::chain_exchange::{
    CHAIN_EXCHANGE_PROTOCOL_NAME, ChainExchangeCodec, ChainExchangeRequest, ChainExchangeResponse,
    HEADERS, MESSAGES,
};
use crate::libp2p::hello::{HELLO_PROTOCOL_NAME, HelloCodec, HelloRequest, HelloResponse};
use crate::networks::NetworkChain;
use crate::shim::bigint::BigInt;
use crate::shim::clock::ChainEpoch;
use crate::utils::version::FOREST_VERSION_STRING;

#[derive(NetworkBehaviour)]
struct ProbeBehaviour {
    /// Responds to inbound `/ipfs/id/1.0.0` queries so peers don't log
    /// "failed to identify peer" against us.
    identify: identify::Behaviour,
    hello: request_response::Behaviour<HelloCodec>,
    chain_exchange: request_response::Behaviour<ChainExchangeProbeCodec>,
}

/// Outbound-only chain-exchange codec. When `discard` is true, the response
/// bytes are read off the wire (so yamux flow-control stays happy and the
/// remote actually completes serialization) but never CBOR-decoded — so the
/// probe does no work proportional to response size. Used for stress-testing
/// the remote without paying parsing cost on our side.
#[derive(Clone, Default)]
struct ChainExchangeProbeCodec {
    discard: bool,
}

#[derive(Debug)]
enum ProbeChainExchangeResponse {
    Parsed(Box<ChainExchangeResponse>),
    Discarded { bytes: u64 },
}

#[async_trait]
impl request_response::Codec for ChainExchangeProbeCodec {
    type Protocol = &'static str;
    type Request = ChainExchangeRequest;
    type Response = ProbeChainExchangeResponse;

    async fn read_request<T>(&mut self, _: &Self::Protocol, _io: &mut T) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        Err(io::Error::other("probe does not serve chain-exchange"))
    }

    async fn read_response<T>(
        &mut self,
        protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        if self.discard {
            let bytes = futures::io::copy(io, &mut futures::io::sink()).await?;
            Ok(ProbeChainExchangeResponse::Discarded { bytes })
        } else {
            ChainExchangeCodec::default()
                .read_response(protocol, io)
                .await
                .map(|r| ProbeChainExchangeResponse::Parsed(Box::new(r)))
        }
    }

    async fn write_request<T>(
        &mut self,
        protocol: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        ChainExchangeCodec::default()
            .write_request(protocol, io, req)
            .await
    }

    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        _io: &mut T,
        _resp: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        Err(io::Error::other("probe does not serve chain-exchange"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Human,
    Json,
}

#[derive(Debug, Clone, Copy, derive_more::Display)]
enum Step {
    #[display("dial")]
    Dial,
    #[display("hello")]
    Hello,
    #[display("chain-exchange")]
    ChainExchange,
}

/// Connect to a single libp2p peer and send Filecoin protocol messages.
///
/// Trailing arguments are a sequence of message subcommands (`hello`,
/// `chain-exchange`, ...) executed in order over a single connection. A
/// `hello` is prepended automatically unless `--no-hello` is set.
#[derive(Debug, Parser)]
#[command(after_long_help = message_help())]
pub struct ProbeCommand {
    /// Peer multiaddress to dial. Must include `/p2p/<peer-id>`.
    #[arg(long)]
    peer: Multiaddr,
    /// Filecoin network the peer belongs to. Used to derive the genesis CID
    /// for the implicit Hello.
    #[arg(long, required = true)]
    chain: NetworkChain,
    /// Skip the implicit Hello prepended to the message sequence.
    #[arg(long)]
    no_hello: bool,
    /// Emit one JSON object per response on its own line, instead of a
    /// human-readable summary.
    #[arg(long)]
    json: bool,
    /// Per-step timeout, in seconds (applies to dial and to each message).
    #[arg(long, default_value_t = 30)]
    timeout_secs: u64,
    /// Override the genesis CID. Required for devnets.
    #[arg(long)]
    genesis_cid: Option<Cid>,
    /// Stress-test mode: read chain-exchange response bytes off the wire but
    /// skip CBOR decoding. The remote still does the full work of serializing
    /// the response; only the probe avoids the parse cost.
    #[arg(long)]
    discard_response: bool,
    /// Sequence of message subcommands to send. Run `probe ... --help` for
    /// the full list of message subcommands and their flags.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 0..)]
    rest: Vec<String>,
}

/// Render the long help for every message subcommand inline, so
/// `probe --help` documents the trailing `hello` / `chain-exchange` syntax
/// and their flags (e.g., `--concurrency`). Computed once on first use.
fn message_help() -> &'static str {
    static HELP: LazyLock<String> = LazyLock::new(|| {
        use clap::CommandFactory as _;
        use std::fmt::Write as _;
        let mut out = String::from("Message subcommands (append after probe options, in order):\n");
        for sub in MessageWrapper::command().get_subcommands_mut() {
            if sub.get_name() == "help" {
                continue;
            }
            let _ = write!(out, "\n{}\n", sub.render_long_help());
        }
        out
    });
    &HELP
}

#[derive(Debug, Subcommand)]
enum MessageCommand {
    /// Send a Hello request.
    Hello(HelloArgs),
    /// Send a ChainExchange request.
    ChainExchange(ChainExchangeArgs),
}

#[derive(Debug, clap::Args)]
struct MessageOpts {
    /// Number of identical concurrent requests to send to the peer.
    #[arg(long, default_value_t = nonzero!(1usize))]
    concurrency: NonZeroUsize,
}

#[derive(Debug, clap::Args)]
struct HelloArgs {
    /// Heaviest tipset CID, repeatable. Defaults to the genesis CID.
    #[arg(long = "tipset-cid")]
    tipset_cid: Vec<Cid>,
    /// Heaviest tipset height. Defaults to 0.
    #[arg(long, default_value_t = 0)]
    height: ChainEpoch,
    /// Heaviest tipset weight as a decimal integer. Defaults to 0.
    #[arg(long, default_value_t = RawBigInt::default(), value_parser = parse_bigint)]
    weight: RawBigInt,
    #[command(flatten)]
    opts: MessageOpts,
}

#[derive(Debug, clap::Args)]
struct ChainExchangeArgs {
    /// Tipset CID to start from. Repeat for multi-block tipsets.
    #[arg(long = "start-cid", required = true)]
    start_cid: Vec<Cid>,
    /// Number of tipsets to request.
    #[arg(long)]
    len: u64,
    /// Include block headers in the response.
    #[arg(long)]
    headers: bool,
    /// Include messages in the response.
    #[arg(long)]
    messages: bool,
    #[command(flatten)]
    opts: MessageOpts,
}

fn parse_bigint(s: &str) -> Result<RawBigInt, String> {
    RawBigInt::from_str(s).map_err(|e| format!("invalid integer `{s}`: {e}"))
}

#[derive(Parser)]
#[command(name = "probe-message", no_binary_name = true)]
struct MessageWrapper {
    #[command(subcommand)]
    msg: MessageCommand,
}

impl ProbeCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let timeout = Duration::from_secs(self.timeout_secs);

        let genesis_cid = self
            .genesis_cid
            .or_else(|| self.chain.genesis_cid())
            .ok_or_else(|| {
                anyhow!(
                    "--genesis-cid is required when probing devnet peers (chain `{}` has no built-in genesis CID)",
                    self.chain
                )
            })?;

        let mut messages = parse_messages(&self.rest)?;
        if !self.no_hello {
            messages.insert(
                0,
                MessageCommand::Hello(HelloArgs {
                    tipset_cid: Vec::new(),
                    height: 0,
                    weight: RawBigInt::default(),
                    opts: MessageOpts {
                        concurrency: nonzero!(1usize),
                    },
                }),
            );
        }
        if messages.is_empty() {
            bail!("no messages to send (try `hello` or `chain-exchange ...`)");
        }

        let target = extract_peer_id(&self.peer)?;

        let format = if self.json {
            OutputFormat::Json
        } else {
            OutputFormat::Human
        };

        let mut swarm = build_swarm(timeout, self.discard_response)?;
        swarm.dial(self.peer.clone())?;
        wait_for(&mut swarm, timeout, Step::Dial, |ev| match ev {
            SwarmEvent::ConnectionEstablished { peer_id, .. } if peer_id == target => {
                Some(Ok(()))
            }
            SwarmEvent::OutgoingConnectionError { error, .. } => {
                Some(Err(anyhow!("dial failed: {error}")))
            }
            _ => None,
        })
        .await?;

        let mut had_error = false;
        for msg in messages {
            match msg {
                MessageCommand::Hello(args) => {
                    let n = args.opts.concurrency.get();
                    let req = build_hello_request(args, genesis_cid)?;
                    let mut outstanding = HashSet::with_capacity(n);
                    for _ in 0..n {
                        let id = swarm
                            .behaviour_mut()
                            .hello
                            .send_request(&target, req.clone());
                        outstanding.insert(id);
                    }
                    let results = collect_responses(
                        &mut swarm,
                        timeout,
                        Step::Hello,
                        outstanding,
                        |ev| match ev {
                            SwarmEvent::Behaviour(ProbeBehaviourEvent::Hello(e)) => classify_rr(e),
                            SwarmEvent::ConnectionClosed { cause, .. } => closed_classify(cause),
                            _ => Classify::Skip,
                        },
                    )
                    .await;
                    for (_, res) in results {
                        match res {
                            Ok(resp) => print_hello(&resp, format),
                            Err(e) => {
                                had_error = true;
                                eprintln!("{}: {e:#}", Step::Hello);
                            }
                        }
                    }
                }
                MessageCommand::ChainExchange(args) => {
                    let n = args.opts.concurrency.get();
                    let req = build_chain_exchange_request(args)?;
                    let mut outstanding = HashSet::with_capacity(n);
                    for _ in 0..n {
                        let id = swarm
                            .behaviour_mut()
                            .chain_exchange
                            .send_request(&target, req.clone());
                        outstanding.insert(id);
                    }
                    let results = collect_responses(
                        &mut swarm,
                        timeout,
                        Step::ChainExchange,
                        outstanding,
                        |ev| match ev {
                            SwarmEvent::Behaviour(ProbeBehaviourEvent::ChainExchange(e)) => {
                                classify_rr(e)
                            }
                            SwarmEvent::ConnectionClosed { cause, .. } => closed_classify(cause),
                            _ => Classify::Skip,
                        },
                    )
                    .await;
                    for (_, res) in results {
                        match res {
                            Ok(resp) => print_chain_exchange(&resp, format),
                            Err(e) => {
                                had_error = true;
                                eprintln!("{}: {e:#}", Step::ChainExchange);
                            }
                        }
                    }
                }
            }
        }

        if had_error {
            bail!("one or more messages failed");
        }
        Ok(())
    }
}

fn parse_messages(rest: &[String]) -> anyhow::Result<Vec<MessageCommand>> {
    use clap::CommandFactory as _;
    let cmd = MessageWrapper::command();
    let names: Vec<&str> = cmd.get_subcommands().map(|s| s.get_name()).collect();

    let mut groups: Vec<Vec<&str>> = Vec::new();
    for tok in rest {
        let tok = tok.as_str();
        if names.contains(&tok) {
            groups.push(vec![tok]);
        } else if let Some(group) = groups.last_mut() {
            group.push(tok);
        } else {
            bail!("expected one of [{}] but got `{tok}`", names.join(", "));
        }
    }
    groups
        .into_iter()
        .map(|g| {
            MessageWrapper::try_parse_from(g)
                .map(|w| w.msg)
                .map_err(Into::into)
        })
        .collect()
}

fn extract_peer_id(addr: &Multiaddr) -> anyhow::Result<PeerId> {
    addr.iter()
        .find_map(|p| match p {
            Protocol::P2p(id) => Some(id),
            _ => None,
        })
        .context("multiaddr must include /p2p/<peer-id>")
}

fn build_swarm(
    request_timeout: Duration,
    discard_response: bool,
) -> anyhow::Result<Swarm<ProbeBehaviour>> {
    let rr_config = request_response::Config::default().with_request_timeout(request_timeout);
    Ok(SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .map_err(|e| anyhow!("tcp transport: {e}"))?
        .with_quic()
        .with_dns()
        .map_err(|e| anyhow!("dns transport: {e}"))?
        .with_behaviour(|keypair| ProbeBehaviour {
            identify: identify::Behaviour::new(
                identify::Config::new("ipfs/0.1.0".into(), keypair.public())
                    .with_agent_version(format!("forest-probe/{}", FOREST_VERSION_STRING.as_str())),
            ),
            hello: request_response::Behaviour::new(
                [(HELLO_PROTOCOL_NAME, ProtocolSupport::Full)],
                rr_config.clone(),
            ),
            chain_exchange: request_response::Behaviour::with_codec(
                ChainExchangeProbeCodec {
                    discard: discard_response,
                },
                [(CHAIN_EXCHANGE_PROTOCOL_NAME, ProtocolSupport::Full)],
                rr_config,
            ),
        })
        .map_err(|e| anyhow!("behaviour: {e}"))?
        .build())
}

fn build_hello_request(args: HelloArgs, genesis_cid: Cid) -> anyhow::Result<HelloRequest> {
    let tip = if args.tipset_cid.is_empty() {
        NonEmpty::of(genesis_cid)
    } else {
        NonEmpty::new(args.tipset_cid).map_err(|_| anyhow!("--tipset-cid resolved to an empty set"))?
    };
    Ok(HelloRequest {
        heaviest_tip_set: tip,
        heaviest_tipset_height: args.height,
        heaviest_tipset_weight: BigInt::from(args.weight),
        genesis_cid,
    })
}

fn build_chain_exchange_request(args: ChainExchangeArgs) -> anyhow::Result<ChainExchangeRequest> {
    let start = NonEmpty::new(args.start_cid)
        .map_err(|_| anyhow!("at least one --start-cid is required"))?;
    let mut options = 0u64;
    if args.headers {
        options |= HEADERS;
    }
    if args.messages {
        options |= MESSAGES;
    }
    if options == 0 {
        bail!("at least one of --headers / --messages is required");
    }
    Ok(ChainExchangeRequest {
        start,
        request_len: args.len,
        options,
    })
}

/// Drive the swarm until `want` returns `Some`, or `timeout` elapses.
/// Used by the dial phase, which has no `OutboundRequestId` to track.
async fn wait_for<T, F>(
    swarm: &mut Swarm<ProbeBehaviour>,
    timeout: Duration,
    step: Step,
    mut want: F,
) -> anyhow::Result<T>
where
    F: FnMut(SwarmEvent<ProbeBehaviourEvent>) -> Option<anyhow::Result<T>>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            bail!("{step}: timed out");
        }
        match tokio::time::timeout(remaining, swarm.select_next_some()).await {
            Ok(ev) => {
                if let Some(r) = want(ev) {
                    return r;
                }
            }
            Err(_) => bail!("{step}: timed out"),
        }
    }
}

/// Classify a single libp2p request_response event for one of our protocols.
/// Reused across both Hello and ChainExchange — they share the same shape.
fn classify_rr<Req, Resp>(ev: request_response::Event<Req, Resp>) -> Classify<Resp> {
    match ev {
        request_response::Event::Message {
            message:
                request_response::Message::Response {
                    request_id,
                    response,
                },
            ..
        } => Classify::Resolved(request_id, Ok(response)),
        request_response::Event::OutboundFailure {
            request_id, error, ..
        } => Classify::Resolved(request_id, Err(anyhow!("{error}"))),
        _ => Classify::Skip,
    }
}

/// What an event-classifier closure says about a swarm event.
enum Classify<T> {
    /// Unrelated event; keep waiting.
    Skip,
    /// One outstanding request resolved (success or failure).
    Resolved(OutboundRequestId, anyhow::Result<T>),
    /// The connection died — fail every still-outstanding request with this
    /// reason.
    ConnectionGone(String),
}

fn closed_classify<T>(cause: Option<libp2p::swarm::ConnectionError>) -> Classify<T> {
    Classify::ConnectionGone(match cause {
        Some(c) => format!("connection closed: {c}"),
        None => "connection closed".to_string(),
    })
}

/// Drive the swarm until every id in `outstanding` has been resolved (or
/// the deadline / connection terminates the batch). Returns one
/// `(id, Result)` entry per request, in the order responses arrived; ids
/// that never resolved get a synthetic timeout/connection-gone error.
async fn collect_responses<T, F>(
    swarm: &mut Swarm<ProbeBehaviour>,
    timeout: Duration,
    step: Step,
    mut outstanding: HashSet<OutboundRequestId>,
    mut classify: F,
) -> Vec<(OutboundRequestId, anyhow::Result<T>)>
where
    F: FnMut(SwarmEvent<ProbeBehaviourEvent>) -> Classify<T>,
{
    let mut results = Vec::with_capacity(outstanding.len());
    let deadline = tokio::time::Instant::now() + timeout;
    while !outstanding.is_empty() {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            for id in outstanding.drain() {
                results.push((id, Err(anyhow!("{step}: timed out"))));
            }
            break;
        }
        match tokio::time::timeout(remaining, swarm.select_next_some()).await {
            Ok(ev) => match classify(ev) {
                Classify::Skip => continue,
                Classify::Resolved(id, res) => {
                    if outstanding.remove(&id) {
                        results.push((id, res));
                    }
                }
                Classify::ConnectionGone(reason) => {
                    for id in outstanding.drain() {
                        results.push((id, Err(anyhow!("{step}: {reason}"))));
                    }
                    break;
                }
            },
            Err(_) => {
                for id in outstanding.drain() {
                    results.push((id, Err(anyhow!("{step}: timed out"))));
                }
                break;
            }
        }
    }
    results
}

fn print_hello(resp: &HelloResponse, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let blob = serde_json::json!({
                "type": "hello",
                "arrival": resp.arrival,
                "sent": resp.sent,
            });
            println!("{blob}");
        }
        OutputFormat::Human => {
            let latency_ms = resp.arrival.saturating_sub(resp.sent) / 1_000_000;
            println!(
                "Hello: latency~{latency_ms}ms (sent={}, arrival={})",
                resp.sent, resp.arrival
            );
        }
    }
}

fn print_chain_exchange(resp: &ProbeChainExchangeResponse, format: OutputFormat) {
    match resp {
        ProbeChainExchangeResponse::Parsed(resp) => print_chain_exchange_parsed(resp, format),
        ProbeChainExchangeResponse::Discarded { bytes } => match format {
            OutputFormat::Json => {
                let blob = serde_json::json!({
                    "type": "chain_exchange",
                    "discarded": true,
                    "bytes": bytes,
                });
                println!("{blob}");
            }
            OutputFormat::Human => {
                println!("ChainExchange: discarded {bytes} bytes (response not parsed)");
            }
        },
    }
}

fn print_chain_exchange_parsed(resp: &ChainExchangeResponse, format: OutputFormat) {
    let total_blocks: usize = resp.chain.iter().map(|t| t.blocks.len()).sum();
    let total_msgs: usize = resp
        .chain
        .iter()
        .map(|t| {
            t.messages
                .as_ref()
                .map_or(0, |m| m.bls_msgs.len() + m.secp_msgs.len())
        })
        .sum();
    match format {
        OutputFormat::Json => {
            let blob = serde_json::json!({
                "type": "chain_exchange",
                "status": format!("{:?}", resp.status),
                "message": resp.message,
                "tipsets": resp.chain.len(),
                "total_blocks": total_blocks,
                "total_msgs": total_msgs,
            });
            println!("{blob}");
        }
        OutputFormat::Human => {
            println!(
                "ChainExchange: status={:?}, message={:?}, tipsets={}, total_blocks={total_blocks}, total_msgs={total_msgs}",
                resp.status,
                resp.message,
                resp.chain.len()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_hello() {
        let msgs = parse_messages(&["hello".into()]).unwrap();
        assert_eq!(msgs.len(), 1);
        assert!(matches!(msgs[0], MessageCommand::Hello(_)));
    }

    #[test]
    fn parses_chained_hello_and_chain_exchange() {
        let cid = "bafy2bzaceabc6h2g6tjvxbqz5h22cnogcrgykvtvkrqsmagaxjxqz4xnflxlw";
        let msgs = parse_messages(&[
            "hello".into(),
            "chain-exchange".into(),
            "--start-cid".into(),
            cid.into(),
            "--len".into(),
            "5".into(),
            "--headers".into(),
        ])
        .unwrap();
        assert_eq!(msgs.len(), 2);
        assert!(matches!(msgs[0], MessageCommand::Hello(_)));
        match &msgs[1] {
            MessageCommand::ChainExchange(args) => {
                assert_eq!(args.len, 5);
                assert!(args.headers);
                assert!(!args.messages);
                assert_eq!(args.start_cid.len(), 1);
            }
            _ => panic!("expected chain-exchange"),
        }
    }

    #[test]
    fn rejects_unknown_leading_token() {
        let err = parse_messages(&["wat".into()]).unwrap_err();
        assert!(err.to_string().contains("expected one of"));
    }

    #[test]
    fn empty_input_yields_empty_vec() {
        let msgs = parse_messages(&[]).unwrap();
        assert!(msgs.is_empty());
    }
}

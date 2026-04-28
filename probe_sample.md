# Forest Dev Probe Command

## Sample usage

```fish
for i in (seq 1 100)
sleep 0.1
forest-dev probe \
          --peer /ip4/209.38.214.74/tcp/44911/p2p/12D3KooWBYicawi7JaFq4Jx3Xb5tebNXUTLFBa86ZTsnk8iJBfzZ \
          --discard-response \
          --chain mainnet \
          chain-exchange \
            --start-cid bafy2bzacealolf2ouaxlqvevkdh4kkz7zstxokd2yzdzs7hi7aqheruadqyr2 \
            --len 900 \
            --headers \
            --messages \
            --concurrency=10000 &
end
wait
```

## Help
```
❯ target/quick/forest-dev probe --help
Open an ephemeral libp2p peer, dial a single peer, and send Filecoin protocol messages (Hello, ChainExchange, ...) over that connection

Usage: forest-dev probe [OPTIONS] --peer <PEER> --chain <CHAIN> [REST]...

Arguments:
  [REST]...
          Sequence of message subcommands to send. Run `probe ... --help` for the full list of message subcommands and their flags

Options:
      --peer <PEER>
          Peer multiaddress to dial. Must include `/p2p/<peer-id>`

      --chain <CHAIN>
          Filecoin network the peer belongs to. Used to derive the genesis CID for the implicit Hello

      --no-hello
          Skip the implicit Hello prepended to the message sequence

      --json
          Emit one JSON object per response on its own line, instead of a human-readable summary

      --timeout-secs <TIMEOUT_SECS>
          Per-step timeout, in seconds (applies to dial and to each message)

          [default: 30]

      --genesis-cid <GENESIS_CID>
          Override the genesis CID. Required for devnets

      --discard-response
          Stress-test mode: read chain-exchange response bytes off the wire but skip CBOR decoding. The remote still does the full work of serializing the response; only the probe avoids the parse cost

  -h, --help
          Print help (see a summary with '-h')

Message subcommands (append after probe options, in order):

Send a Hello request

Usage: hello [OPTIONS]

Options:
      --tipset-cid <TIPSET_CID>
          Heaviest tipset CID, repeatable. Defaults to the genesis CID

      --height <HEIGHT>
          Heaviest tipset height. Defaults to 0

          [default: 0]

      --weight <WEIGHT>
          Heaviest tipset weight as a decimal integer. Defaults to 0

          [default: 0]

      --concurrency <CONCURRENCY>
          Number of identical concurrent requests to send to the peer

          [default: 1]

  -h, --help
          Print help


Send a ChainExchange request

Usage: chain-exchange [OPTIONS] --start-cid <START_CID> --len <LEN>

Options:
      --start-cid <START_CID>
          Tipset CID to start from. Repeat for multi-block tipsets

      --len <LEN>
          Number of tipsets to request

      --headers
          Include block headers in the response

      --messages
          Include messages in the response

      --concurrency <CONCURRENCY>
          Number of identical concurrent requests to send to the peer

          [default: 1]

  -h, --help
          Print help
```

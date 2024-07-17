# Healthcheck 🩺

It is helpful for monitoring and alerting systems to check if the node is up and
running out of the box. Forest ships with a set of healthcheck endpoints that
can be used to monitor the node status and perform actions based on the results.

## Endpoints

All healthcheck endpoints operate on port `2346` by default. This behaviour can
be changed via the `--healthcheck-address` flag. All endpoints expose a
`verbose` optional query parameter that can be used to get more detailed
information about the node's health status.

Endpoints return a `200 OK` status code if the node is healthy and a
`503 Service Unavailable` status code if the node is not healthy.

### `/livez`

Liveness probes determine whether or not an application running in a container
is in a healthy state. The idea behind a liveness probe is that it fails for
prolonged period of time, then the application should be restarted. In our case,
we require:

- The node is not in an error state (i.e., boot-looping)
- At least 1 peer is connected (without peers, the node is isolated and cannot
  sync)

If any of these conditions are not met, the node is **not** healthy. If this
happens for a prolonged period of time, the application should be restarted.

Sample _lively_ response:

```
❯ curl "http://127.0.0.1:2346/livez?verbose"
[+] sync ok
[+] peers connected⏎
```

Sample _not lively_ response:

```
❯ curl "http://127.0.0.1:2346/livez?verbose"
[+] sync ok
[!] no peers connected
```

### `/readyz`

Readiness probes determine whether or not a container is ready to serve
requests. The goal is to determine if the application is fully prepared to
accept traffic. In our case, we require:

- The node is in sync with the network
- The current epoch of the node is not too far behind the network
- The RPC server is running
- The Ethereum mapping is up to date

If any of these conditions are not met, the node is **not** ready to serve
requests.

Sample _ready_ response:

```
❯ curl "http://127.0.0.1:2346/readyz?verbose"
[+] sync complete
[+] epoch up to date
[+] rpc server running
[+] eth mapping up to date⏎
```

Sample _not ready_ response:

```
❯ curl "http://127.0.0.1:2346/readyz?verbose"
[!] sync incomplete
[!] epoch outdated
[+] rpc server running
[!] no eth mapping⏎
```

### `/healthz`

This endpoint is a combination of the `/livez` and `/readyz` endpoints, except
that the node doesn't have to be fully synced. Deprecated in the Kubernetes
world, but still used in some setups.

Sample _healthy_ response:

```
❯ curl "http://127.0.0.1:2346/healthz?verbose"
[+] sync complete
[+] epoch up to date
[+] rpc server running
[+] sync ok
[+] peers connected⏎
```

Sample _unhealthy_ response:

```
❯ curl "http://127.0.0.1:2346/healthz?verbose"
[!] sync incomplete
[!] epoch outdated
[+] rpc server running
[+] sync ok
[!] no peers connected⏎
```
